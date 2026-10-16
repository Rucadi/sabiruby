# 2026-10-17 rc2-leftovers5: rc2-ivar の残り（R1〜R4）

計画: `../plans/rc2-leftovers5-plan.md`。差の出どころは `2026-10-17-rc2-ivar.md`「気づいた点」。
方針は `../design/compatibility.md`（本家に合わせる）。正解は 4.1.0-rc2（Docker
`kishima/mruby:4.1.0-rc2`、ソースは `/home/kishima/book/ref/mruby` の `c17ffcc24`）。base は `1c3be05`。

4 つとも「本家に合わせる」側で、本家を不具合とみなして外れたものは無い（README の Known deviations は
増えない）。

## R1. 多重代入の右辺と block の引数

### rc2 を読む

SabiRuby の compiler は rc2 の codegen を C のまま建てたものなので、命令列は同じ。差は VM の側にしか
ありえない。rc2 の `src/vm.c` で `mrb_array_p` を使う所を探した（`:2160`、`:2514`、`:2630`、`:4332`、
`:4369`、`:4395`）。多重代入と block に関わるのは 3 つ。

- `OP_AREF`（`:4366`）: `mrb_array_p(v)` が偽なら、`c == 0` のときその値、それ以外は nil。`to_ary` は
  呼ばない。
- `OP_APOST`（`:4390`）: 偽なら `ary_new_from_regs(mrb, 1, a)`、つまり値 1 つの Array として分ける。
- `OP_ENTER` の block の引数（`:2630`）: `len > 1 && argc == 1 && mrb_array_p(argv[0])` のときだけ展開。

`mrb_array_p` は `include/mruby/value.h:315` で `mrb_type(o) == MRB_TT_ARRAY`。Array のサブクラスは
真、Struct と Data（`MRB_TT_STRUCT`）は偽。

### SabiRuby を読む

`Op::Aref`（`src/vm.rs:4700`）は `heap.array(o)` が `None` の Obj について `c` を見ずに `reg!(b)` を返して
いた。即値の腕だけ `c == 0` を見ていた。だから `a, b, c = MyAry.new` は 3 つともオブジェクトになり、
`a, b = 1.0` は合っていた（即値なので）。`to_ary` を呼んでいたのではない。

もう 1 つ、Struct と Data は `ObjKind::Array` の形を借りている（`ext_struct.rs:273`、`ext_data.rs`）ので、
`heap.array(o)` が `Some` を返し、`OP_AREF`・`OP_APOST`・`op_enter` の展開（`vm.rs:5648`）で Array として
分けられていた。rc2 との比較（スクラッチ `l5/masgn2.rb`）:

| 式 | rc2 | SabiRuby（前） |
|---|---|---|
| `a, b = Struct.new(:x, :y).new(1, 2)` | `[#<struct>, nil]` | `[1, 2]` |
| `a, *b = s` | `[#<struct>, []]` | `[1, [2]]` |
| `[s].each { \|a, b\| }` | `[#<struct>, nil]` | `[1, 2]` |
| `a, b = Data のオブジェクト` | `[#<data>, nil]` | `[1, 2]` |
| `a, b = Set[1, 2]`／`{1 => 2}`／`1..2`／`"ab"` | 値と nil | 値が 2 つ |
| `class SubA < Array`、`a, b = SubA[1, 2]` | `[1, 2]` | `[1, 2]` |

計画は「同じ仕組みで違っていれば合わせる」なので、Struct・Data も同じ `mrb_array_p` の判定の話として
範囲に入れた。

### 写し方

Struct・Data と本物の Array を見分ける印が heap に無い。考えた案:

1. `OP_AREF` で `ObjKind::Array` のときにクラスの鎖をたどって Struct の子孫か見る。`a, b = f()` のたびに
   本物の Array でも鎖をたどることになる。捨てた。
2. Q1 の `no_ivars` を流用する（Array の形では `iv_p` が常に偽なので今は意味が無い）。印 1 つで済むが、
   「iv を持てない」と「Array でない」という別の意味を 1 つの bool に重ねることになる。捨てた。
3. `HeapObject` に `tt_struct: bool` を足す。`class`（4 バイト）と 3 つの bool の後の詰め物に収まる。
   割り当てのときに立て、読むのは既に手元にある `HeapObject` だけ。これを採った。

変えたところ:

- `HeapObject::tt_struct`（`src/object.rs:490`）と `Heap::plain_array`（`:800` 付近、`mrb_array_p`）。
- 立てる所: `struct_new`（`ext_struct.rs:273`）、Data の 2 か所（`ext_data.rs` の `new_instance` に
  まとめた）、`Object#dup`（`builtins/object.rs`、`no_ivars` と同じく写す。`clone` は `dup` を通る）。
- 読む所: `Op::Aref`（`c == 0` のときだけ値、他は nil）、`Op::Apost`、`op_enter` の展開。

`Vm::ary`／`ary_vals`（ホストの API）は変えていない。Struct のメソッド自身がこれで中身を読むので、
Struct を除くとそちらが壊れる。splat（`Vm::to_array`、`mrb_ary_splat`）は `ary_vals` のままにした:
Struct は `to_a` で同じ中身を返すので、`[*s]`・`a, b = *s` の答えは rc2 と同じ（確かめた）。

### 確かめ

スクラッチ `l5/masgn.rb`（`to_ary` を持つ値、24 行）、`masgn2.rb`（Struct・Data・Set・Hash・Range・
String・Array のサブクラス）、`masgn3.rb`（Struct のサブクラス、dup、凍結、ネスト、`proc.call`、
`yield`、splat）を rc2 と比べ、直した後は 1 バイトも違わない。それをまとめたのが
`tests/custom/masgn_not_array.rb`（`.expected` は rc2 の出力。`tools/custom.sh` の `.rc.out` と一致）。

### 本との対応

`book_mruby3/contents/overview.re:460-461`「暗黙の型変換（`to_int`、`to_str`、`to_ary`）をしない」
「多重代入の右辺の`to_ary`も呼ばれません」。rc2 は `a, b, c = MyAry.new` が `[MyAry, nil, nil]`（`to_ary`
を呼ばない）。直した後の SabiRuby も同じ。本文と rc2 と SabiRuby がそろった。本文は触っていない。
CRuby（3.2.6）は `to_ary` を呼んで `[1, 2, 3]`、Struct は `to_ary` が無いので `[#<struct>, nil]`。
book の `docs/notes/language-differences.md` に 1 段落足した。

## R2. `remove_instance_variable` の名前の検査

rc2 の `mrb_obj_remove_instance_variable`（`src/kernel.c:594`）は `mrb_get_args("n")` の後に
`mrb_iv_name_sym_check`（`src/variable.c:829`）、それから `mrb_iv_remove`、undef なら
`mrb_name_error(mrb, sym, "instance variable %n not defined")`。名前の検査は型と凍結の検査より先。
`mrb_iv_name_sym_check` を呼ぶのは他に `prepare_ivar_name`（`src/class.c:3103`、`attr_*`）と
mruby-metaprog の `instance_variable_get`／`set`／`defined?`（`metaprog.c:44`、`:74`、`:105`）。

SabiRuby: `ext_metaprog.rs` の 3 つは既に `iv_check` を通っていた（`attr_*` は `object.rs:583`）。
`builtins/object.rs:62` の `remove_instance_variable` だけが `sym_arg` のままで、"not defined" の
NameError も `vm.raise(name_error, …)` で `name` が nil だった。`iv_check` を `pub(crate)` にして通し、
"not defined" は `vm.name_error(n, …)` にした。

スクラッチ `l5/ivname.rb`: 7 つの名前（`:x`、`:"@"`、`:"@1"`、`:@@a`、`:"@a?"`、`:@a`、`"@a"`）× 4 つの
メソッド、`e.name`、`"s"`・`1`・凍結した Object での検査の順、Symbol でない引数。前は
`remove_instance_variable` の 5 行と `name=` の列、凍結した Object の FrozenError（rc2 は名前の
NameError が先）が違った。後は一致（非 ASCII の名前の行は、Symbol#inspect が UTF-8 のビルドかで
変わるだけなのでテストから外した。SabiRuby の既定は utf8 で、rc2 の utf8 イメージと同じ）。
`tests/custom/remove_ivar_name.rb`。

`builtins/object.rs:73-76` の `instance_variable_get`／`set`／`defined?`／`instance_variables` は
`ext_metaprog::init` が同じ名前で上書きするので使われていない（`method(:instance_variable_get).owner`
は Kernel で、名前の検査が効いている）。範囲の外なので backlog に書いて残した。

## R3. Regexp の `instance_variables` の順

計画の前提は「`ext_regexp.rs` の書く順を rc2 に合わせる」だったが、rc2 の `re_initialize`
（`mrbgems/mruby-regexp/src/regexp.c:152-153`）は SabiRuby と同じく `@source`、`@flags` の順に書いている。
それでも rc2 の答えは `[:@flags, :@source]`。

理由は iv の入れ物。rc2 の `src/variable.c` は、普通のオブジェクト（`MRB_TT_OBJECT`）だけ shape
（隠しクラス）で書いた順を保ち（`shaped_iv_foreach`）、それ以外（Hash、Exception、CDATA、クラス）は
`iv_tbl` にシンボルの番号の順で並べる（`:73` の二分探索で挿入位置を決める）。Regexp は CDATA なので
後者で、presym の `@flags` が `@source` より小さい番号を持つ。確かめ（スクラッチ `l5/ivord.rb`）:

| | rc2 | SabiRuby |
|---|---|---|
| Object に `@zzz`、`@aaa`、`@source`、`@flags` | 置いた順 | 置いた順 |
| `{}` に `@source`、`@flags` | `[:@flags, :@source]` | `[:@source, :@flags]` |
| Exception に `@zzz`、`@aaa` | `[:@zzz, :@aaa]`（スクリプトの中で先に intern された方が小さい） | 同じ |

つまり差は Regexp に限らず「OBJECT 以外の iv の順」の話。全部を写すには rc2 と同じシンボルの番号
（presym の並びと intern の順）が要り、SabiRuby のシンボル表は作りが違うので、ここではやらない
（backlog）。計画が求めた Regexp 自身の 2 つは、`ext_regexp.rs` で `@flags` を先に書けば rc2 の答えに
なるので、そうした（理由をコメントに書いた）。`@named_captures` はその後ろで、rc2 と同じ。MatchData は
rc2 も SabiRuby も `[]`。

合わない所が 1 つ残る: `class R2 < Regexp; def initialize(*a); @pre = 1; super; end; end` は rc2 が
`[:@flags, :@source, :@pre]`（`@pre` は実行時に intern されたので番号が大きい）、SabiRuby は
`[:@pre, :@flags, :@source]`。上の一般の話の一例として backlog に入れた。
`tests/custom/regexp_ivar_order.rb`（`# regexp-only:`）はこの行を入れずに作った。

## R4. `Kernel#instance_variable_names`

rc2 には無い（`grep -rn instance_variable_names` が `ref/mruby` で 0 件）。使っている所を探した:

```
grep -rn instance_variable_names kishima/sabiruby kishima/rubevy kishima/rubevy_games \
  kishima/sabiruby-playground kishima/mruby-porting-kit book_mruby3   （target/ を除く）
```

当たったのは `sabiruby/src/builtins/kernel.rs:32`（定義そのもの）、`docs/verification/coverage.md:256`
（`tools/coverage.sh` の生成物。「Do not edit」で 2026-09-16 の日付なので、作り直さず残した）、
`docs/worklog/2026-10-17-rc2-ivar.md` と計画書だけ。`tests/`、`bench/`、`src/mrblib`、rubevy、
rubevy_games、playground、移植キット、本では 0 件（`.mrb` も `grep -l` で 0 件）。使われていないので
消した。`Object.new.instance_variable_names` は NoMethodError になる。

## 確認

- rc2 の mrbtest（`tools/mrbtest.sh`、Docker）: 既定 2346/2508、`--bytes` 2269/2453、`--no-regexp`
  1981/2012。3 つとも前と同じで、生成された表は日付の行しか変わらなかったので戻した。
- `cargo test --workspace`: 39 スイート、275 passed、0 failed（`custom_cases` は 1 つのテストで
  `tests/custom/*.rb` を全部回すので、新しい 3 件で数は増えない）。
- `cargo test -p sabiruby --no-default-features --features "std utf8"`: 161 passed、0 failed。
- `tools/check_no_std.sh --features regexp`、`--features utf8`: no_std OK。
- `cargo build -p sabiruby --lib --target wasm32-unknown-unknown`: 通る。
- `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --workspace --lib`、`-p sabiruby-cli --bins`: 通る。
- MSRV: `cargo +1.85 check --workspace --all-targets`（別の target）: 通る。
- `unsafe` は増やしていない。`Vm: Send + Sync`（`tests/send_sync.rs` は上に入る）。新しい数は無い
  （`m_masgn` の 200_000 は `m_loop_only.rb` と同じ回数。差し引くため）。

## 性能

base は `1c3be05`（`git worktree add --detach` の別ディレクトリ、別の `CARGO_TARGET_DIR`、md5
`2b505482…`）、new は作業ツリー（md5 `bac36dec…`）。`vmstat` で 93〜99% idle。この日の機械は前回
（rc2-ivar の全体 45.6 秒）より遅く（51.8 秒）、ぶれも大きかった。

`tools/bench_ab.sh --core 2 --runs 5`（全体）: all 51846 → 51444 ms（-0.8%）。カテゴリは whole program
-0.7%、data structures -0.8%、instruction loop -1.5%、calls +1.4%、memory -6.2%。多重代入を使うベンチ
（`bm_mandel_term`、`bm_ao_render`、`ds_hash`、`vmo_objects`、`vm_optimization_bench`）は -0.9%、-2.0%、
-1.1%、-2.7%、+0.5%。

ぶれの大きかったものと calls を 10 巡で取り直した: `call_block_yield` +1.8%、`call_args` +2.8%、
`call_kwargs` -0.4%、`bm_ao_render` +0.1%、`ds_hash` +4.0%、`vmo_objects` -0.8%、`bm_so_lists` -6.8%。
もう一度 10 巡: `call_args` +1.3%、`call_block_yield` +7.2%、`ds_hash` -1.2%。`call_block_yield` は
`{ |x| }`（引数 1 つ）なので、変えた展開の分岐（`len > 1`）を通らない。20 巡で取り直すと best +1.4%、
median は -0.9%（1041.9 → 1033.0）で、ぶれの範囲と読む。

`bench/` に OP_AREF／APOST／ENTER の展開だけを回すものが無いので、`bench/micro/src/m_masgn.rb` を
足した（`a, b = pair`、`c, *d = trio`、`proc { |x, y| }.call(pair)`、`m_loop_only` と同じ 200_000 回）。
`tools/ab_micro.sh --runs 10 --core 2`: `m_masgn` 53.46 → 51.57 ms（-3.5%）、`m_loop_only` 8.86 → 8.90
（+0.4%）。ループを引くと 44.6 → 42.7。`plain_array` は `array` に bool を 1 つ読むのを足しただけで、
速くなる理由は無いので、遅くなっていないと読む。他の micro は触っていない
所が ±9% で動いた（`m_ary_dup` +9.1%、`m_ary_set` -7.7%）ので、この日の機械のぶれ。

結果の tsv はコミットしていない（最近の記録と同じ。数はここに書いた）。

## edge_compare

`tools/edge_compare.sh`: C の fixture 無し pass 858、fail 33、error 34、skip 2（passed 30 ファイル）、
あり pass 883、fail 43、error 31、skip 2（passed 30）。前（`8c32b3b` の JSON）と同じで、変わったのは
commit・日時・`elapsed_seconds` だけだったので、JSON とページは戻した。

## 気づいた点

- **VM / rc2 との差（記録だけ）**: Array のメソッドが Struct・Data を Array として受け取る。スクラッチ
  `l5/masgn3.rb` の後半: `[1] + s`、`[1, 2].concat(s)`、`[3, 4].replace(s)`、`[3, 4].product(s)` は rc2 が
  TypeError、SabiRuby は中身を使う。`[s].flatten` は rc2 が `[s]`、SabiRuby は `[1, 2]`。`[1, 2] == s` は
  rc2 false、SabiRuby true。`[3, 4] <=> s` は rc2 nil、SabiRuby 1。`Array(s)` は rc2 `[1, 2]`、SabiRuby は
  struct そのもの。原因は R1 と同じ（Struct が `ObjKind::Array` の形）で、直すなら各メソッドの引数を
  `Heap::plain_array` で読む。backlog に入れた。
- **VM / rc2 との差（記録だけ）**: OBJECT 以外の `instance_variables` の順（R3 の節）。backlog。
- **VM（内部）**: `builtins/object.rs:73-76` の上書きされて使われない 4 つ（R2 の節）。backlog。
- **生成物**: `docs/verification/coverage.md` は 2026-09-16 のままで、`instance_variable_names` が
  まだ載っている。作り直すと他の差分も大きく出るので触っていない。次に `tools/coverage.sh` を
  回すときに消える。
- **本の素材**: Struct が Array の形を借りると、Ruby から見える型の判定（`is_a?`）はクラスで正しく
  ても、VM の命令の中の「Array か」の判定が形で答えてしまう。rc2 は型タグ（`MRB_TT_STRUCT`）を別に
  持つのでこの問題が起きない。
