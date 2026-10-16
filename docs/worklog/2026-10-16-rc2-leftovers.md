# 2026-10-16 4.1.0-rc2 との残りの差（L1〜L5）

計画: `docs/plans/rc2-leftovers-plan.md`。`2026-10-16-follow-master.md` の「戻した後も残る、前からの
rc2 との差」に挙げた 5 つを、本家 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`）の答えに合わせた。

進め方は 1 件ごとに同じ: 数行の Ruby（スクラッチの `t/*.rb`）を rc2 の `mruby` と SabiRuby で走らせて
出力を並べ、rc2 のソース（イメージの `/usr/src/mruby` を `tar` で取り出したもの）でその答えを作って
いる関数を読み、写す。読んだ場所は rc2 のツリーの行番号で書く。最後に断片をまとめて
`tests/custom/rc2_leftovers.rb` にした（期待値は rc2 の出力そのもの。`.expected` と `.rc.out` が同じ）。

## L1. 定数を、走っている proc から引く

### rc2 の作り

`OP_GETCONST`（`src/vm.c:3350`）は `mrb_vm_const_get`（`src/variable.c:1353`）を呼ぶ。読む順は:

1. `c = MRB_PROC_TARGET_CLASS(ci->proc)`。**フレームの target class（`ci->u.target_class`）ではなく
   proc のもの**。`MRB_PROC_TARGET_CLASS`（`include/mruby/proc.h:111`）は、環境（REnv）を持つ proc
   なら環境の `c`、持たない proc なら `e.target_class`。
2. `c` の表だけを見る（`iv_get`。祖先は見ない）。
3. `proc->upper` を外へたどり、各 proc の target class の表だけを見る（ここも祖先は見ない）。
4. `c` が特異クラスなら、`const_get_nohook(c, skip=TRUE)` で `c` の祖先を見て、次に `__attached__`
   を特異クラスでなくなるまでたどり、クラスかモジュールに着いたら `c` をそれにする。
5. `const_get(c, sym, skip=TRUE)`（`src/variable.c:1278`、`1303`）: `c->super` から上を全部（Object も
   BasicObject も）。`c` がモジュールなら、その後 Object から上をもう一度。無ければ `c` の
   `const_missing`。

`class_eval` のブロックは `mrb_yield_with_class`（`src/vm.c:1982` → `yield_with_attr`）で走り、
`ci->u.target_class` には与えたクラスが入るが、ブロックの proc は呼び出し元で作られた環境を持って
いるので、`MRB_PROC_TARGET_CLASS` は書かれた場所のクラスを答える。だから `m.class_eval { K2 }` は
`m` を見ない。ただし、そのブロックの**中で作られた**ブロック（`m.class_eval { [1].map { K2 } }` の
内側）は、`class_eval` のフレームで作られた環境を持ち、その環境の `c` はフレームの target class
（= `m`）なので、`m` から引く。rc2 の出力でもそうなる（`[:lex, [:recv], :lex, :lex, :lex]`）。
`define_method` も同じで、`define_method_m`（`src/class.c:4163`）は proc を写すだけで、環境を持つ
proc の target class は変えない（`mrb_define_method_raw` は `MRB_PROC_ENV_P` でない proc にだけ
`MRB_PROC_SET_TARGET_CLASS` する。`src/class.c:1054`）。

### SabiRuby の前の作り

`Vm::const_lookup`（`src/vm.rs`）は 1) フレームの `target_class` とその祖先、2) proc の `upper` を
たどって各 `ProcData::target_class` とその祖先、3) Object、の順だった。違いは 3 つ:

- 1 の「フレームの target class」が `class_eval` の受け手になる（計画の L1 そのもの）。
- どの段でも祖先を見るので、`module M; X = :m; class B < A; X; end; end` で、rc2 は字句の `M::X`
  （`:m`）、SabiRuby は継承の `A::X`（`:a`）を答えた。CRuby も `:m`。
- `const_missing` を送る先がフレームの target class で、`def self.f; NOPE; end` では特異クラスに
  送っていた。rc2 は `__attached__` をたどった `F` に送る（`F.const_missing` が呼ばれる）。

### 直し

`Vm::const_search`（新）に rc2 の 1〜5 をそのまま写し、`OP_GETCONST` の `const_lookup` と
`defined?` の `const_lookup_noraise`（rc2 の `mrb_vm_const_get_noraise`、`src/variable.c:1393`。
`Kernel#__defined_const?` から呼ばれる）の両方をそこに通した。`OP_GETCONST` はフレームの target
class を使わなくなった。

rc2 にある定数のキャッシュ（`MRB_CONST_CACHE_SIZE`、irep と名前で引く。`src/vm.c:3351`）は写して
いない。

断片の比較（スクラッチ `t/c.rb`、25 行）は直す前 12 行違い、直した後は全行同じ。

### 最初の形は遅かった、と、proc が自分のクラスを持つ形

最初は `MRB_PROC_TARGET_CLASS` をそのまま写し、`Vm::proc_target_class` が「環境があれば
`Env::target_class`、無ければ `ProcData::target_class`」を答えていた。答えは合ったが、
`tools/bench_ab.sh`（P コア 2 番、5 回）で、同じ日の A/A（base 同士、全体 -0.1%、1 本ごとには
-2.3〜+3.5%）に対して `vmo_arith` +10.0%、`vmo_dispatch` +8.1%、`vm_optimization_bench` +6.4% と
線を越えた（全体 +2.9%、1 巡目。2 巡目以降は打ち切った）。`vmo_arith` は
`measure(...) do ... while i < N ... end` で、ループ 1 周ごとに `GETCONST N` がある。前の作りは
フレームの target class の表を 1 回引くだけだったのが、proc と環境の 2 回のヒープの参照が足された。

試した案 1: フレームの target class を使い、`vis_break` のフレーム（`class_eval` などで入った
フレーム、rc2 の `mrb_yield_with_class`）だけ proc から読む。`debug_assert!` で「それ以外のフレーム
ではフレームの class と proc の class が同じ」を確かめながら `cargo test` を（debug で）回すと、
2 種類の反例が出た: (a) include したモジュールのメソッドのフレームは、class が include クラス
（iclass）で proc は モジュール（`Enumerable#inject`）。iclass → モジュールに写せば同じ。
(b) `define_method` のブロックをメソッドとして呼んだフレームは、class がメソッドを見つけたクラス、
proc は書かれた場所のクラス（`NN::AA.new.dmx`）。(b) をフレームの情報だけで見分ける手が無く
（`CallInfo` に旗を足すと公開の構造体が変わる）、捨てた。速さは `vmo_arith` +1.3%、`vmo_dispatch`
+0.2% に戻っていたので、参照 2 回が原因という見立ては合っていた。

採った案: **proc が `MRB_PROC_TARGET_CLASS` を自分で持つ**。ブロックはそのフレームの class で作られ、
それは捕まえる環境が作られたときの class と同じ（環境は同じフレームで作られ、フレームの class は
途中で変わらない）。違っていたのは 2 か所だけ:

- `define_method`／`define_singleton_method` が、ブロックの `ProcData::target_class` を受け手に
  書き換えていた。rc2 の `mrb_define_method_raw` は環境の無い proc にしか target class を書かない
  （`src/class.c:1054`）ので、環境を持つ proc には書かないようにした。環境を持つ proc をメソッドと
  して呼ぶ道（`call_method_proc`、`exec_method_proc`、SEND の `MethodRef::Ruby`）はどれも
  `ProcData::target_class` を読まないので、変わるのは定数とクラス変数の探索だけ。クラス変数は
  rc2 の `mrb_vm_cv_get` も `MRB_PROC_TARGET_CLASS` から引くので、これで rc2 に合った（前は
  `B.define_method(:cv) { @@v }` が B の `@@v` を読んでいた。rc2 は書かれた A のもの）。
- `instance_eval`／`class_eval` に渡した文字列の proc（`ext_eval.rs` の `eval_proc`）。rc2 の
  mruby-eval は `MRB_PROC_SET_TARGET_CLASS` で与えたクラスを書き込む。環境を持つ proc なので、
  書かれるのは**呼び出し元と共有している環境**の `c` で、その後の呼び出し元の `def` まで行き先が
  変わる（`def f; M.class_eval("X"); end` の後のトップレベルで、rc2 は `f` が無いと言う）。
  ここでは文字列の proc 自身に書く（`Vm::run_eval`）。最初の形では `M.class_eval("X")` が
  トップレベルの `X` を答えていて（rc2 は `M::X`）、それもこれで直った。

`proc_target_class` は `ProcData::target_class` を読むだけになり、`GETCONST` で足される参照は proc の
1 回になった。それでも 2 巡（交互）で `vmo_arith` +6.4%／+6.0%（2 巡目は向きを入れ替えて base が
5.7% 速い）、`vmo_dispatch` +4.4%／+4.9%、`vmo_index` +3.4%／+4.3%、全体 +1.8%／+1.8% と、A/A の線
（1 本ごとに ±3.5% まで）をまだ越えていた。参照 1 回でもループ 1 周の数 % に当たる。

### 案 1 に戻って、フレームに旗を 1 つ

そこで案 1（フレームの class をそのまま使う）に戻し、見分けられなかった (b) のために
`CallInfo::method_block` を足した。SEND が `MethodRef::Ruby` のフレームを積むところ
（`op_send` の末尾）で proc に環境があるとき、つまりブロックをメソッドとして呼ぶときだけ真。
`Vm::const_class` は、`vis_break` か `method_block` のフレームと、フレームの class が include クラス
か origin のフレームだけ proc を読み、それ以外はフレームの class をそのまま使う。その「それ以外」で
フレームの class と proc の class が同じことは `debug_assert!` で確かめ続けている（debug の
`cargo test --workspace` が mrbtest の 111 ファイル（`tests/mrbtest/*.mrb`、`assert.mrb` を除く）も回すので、全部の上で成り立っている）。
`CallInfo` は `pub mod vm` から見える公開の構造体なので、フィールドが増えたことを CHANGELOG の API に
書いた。`bool` 1 つで、既にある `bool` の後ろに入る。

include クラスの扱いは、この旗の版で debug_assert が見つけた 3 つ目の反例から決めた
（`Enumerable#max`、`#inject`）。rc2 の SEND はフレームの class に**メソッドを見つけたクラス**を入れる
（`src/vm.c:3654`、`mrb_vm_find_method` の第 3 引数）。include したモジュールのメソッドなら include
クラス（iclass）。メソッドの proc の class はモジュール（`mrb_define_method_raw` が書く）なので、
メソッドの本体の `GETCONST` はモジュールから引く。ところがその中で作ったブロックの環境の `c` は
フレームの class、つまり iclass で、ブロックの中の `GETCONST` は iclass から引く。iclass の
`iv` はモジュールのものを共有しているので最初の表は同じだが、祖先は iclass の `super`、つまり
**include したクラスの祖先**になる。rc2 で確かめた:

```ruby
module Mx; MC = :mx; def blk; [1].map { [MC, SUPC] }; end; def bare; SUPC; end; end
class Sup; SUPC = :sup; end
class Inc2 < Sup; include Mx; end
Inc2.new.blk   # => [[:mx, :sup]]   ブロックからは Sup の定数が見える
Inc2.new.bare  # => NameError (uninitialized constant Mx::SUPC)   メソッドの本体からは見えない
```

SabiRuby の proc も同じ形になっている（ブロックはフレームの class = iclass を持つ）ので、表を引く所
（`Vm::own_const`）で iclass ならモジュールの表を読むようにした。origin（prepend したクラスの元の
メソッド表を持つ iclass）も rc2 ではクラスの表を共有していて（`prepend` したモジュールのメソッドの
ブロックから、クラスの定数が見える。rc2 `[:pre]`）、前の SabiRuby は `NameError` だった。
`Vm::const_get` の祖先の歩きも origin をクラスの表に写すようにした（`iclass_of` は前から写していた）。

### 命令ループの腕の形

この形（旗つき）の 2 巡は、`vmo_arith` +4.9%／+4.3%、`vmo_dispatch` +2.8%／+3.5%、そして定数を
1 つも使わない `bm_so_mandelbrot` +10.6%／+11.9% と `bm_mandel_term` +10.2%／+10.5% だった
（全体 +2.1%／+1.7%。2 巡目は向きを入れ替えたので、base が速い割合から B に対する増分に直した）。mandelbrot は `GETCONST` を通らないので、これは命令ループのコードの配置の
差（`const_class` が `#[inline]` で `OP_GETCONST` の腕に入り、ループの関数が大きくなった）と読んだ。
`docs/design/optimizations.md` と `2026-09-16-perf3.md` にある、この PC の「配置で 10% 動く」話と
同じ種類。

- `const_lookup` を `#[inline(never)]` にして、腕を呼び出し 1 つにした（前の作りも腕は
  `const_lookup` の呼び出しだった）: mandelbrot 2 本は -1.1%／-1.0% に戻ったが、`vmo_arith` +4.5%、
  `vmo_dispatch` +3.4% が残った（5 回、1 巡）。
- さらに、`const_lookup` の中で「`vis_break` でも `method_block` でもなく、フレームの class が
  include クラスでも origin でもない」ときはフレームの class の表を直接引いて、当たればすぐ返す
  ようにし、残り（proc を読む、外へたどる、祖先、`const_missing`）を `#[cold]` の
  `const_lookup_slow` に分けた: `vmo_arith` -2.7%、`vmo_dispatch` -0.8%、`vmo_index` -0.8%、
  `vmo_calls` +0.7%、`bm_so_mandelbrot` -0.2%（5 回、1 巡）。`bm_mandel_term` だけ +12.0% と出たが、
  18 ms の短いもので、11 回ずつ向きを入れ替えて 4 巡取り直すと -0.1%、-1.8%、-0.1%、+0.1% だった
  （遅い時間帯に当たっただけ）。この直接の当たりでも、フレームの class と proc の class が同じことを
  `debug_assert!` で確かめている。

### 定数を使わないベンチマークの 2〜5%

その形（v6）の 2 巡で、全体は -0.2%／-0.5% だったが、`call_kwargs` +5.4%／+4.6%、`bm_ao_render`
+3.1%／+3.2%、`app_json_hash` +3.4%／+2.4% が両方の巡で同じ向きに出た。9 回ずつ向きを入れ替えて
取り直しても `call_kwargs` +5.3%／+3.5%。`call_kwargs` はループの中に `GETCONST` が 1 つも無い
（`bench/src/call_kwargs.rb`）。順に外して確かめた（どれも 9 回、交互）:

- `ProcData` を読む SEND の変更（`method_block` を求める所）を外す: +5.0%／+4.7%。原因ではない。
- `CallInfo::method_block` のフィールドごと外す（`size_of::<CallInfo>()` は前後とも 56 で変わら
  ない）: +5.9%／+6.8%。原因ではない。
- `OP_GETMCNST` の腕を前の形に戻す: +7.1%／+11.6%。原因ではない。
- `src/builtins/array.rs` を前のものに戻す: +2.1%／+2.4%、もう 1 巡 +1.5%（逆向き）／+2.7%。
  同じ木に今の `array.rs` を戻すと +4.8%、+4.4%。`call_kwargs` は `Array#[]=` を呼ばない。
- 対照として、base に `frozen_error` を `#[cold] #[inline(never)]` にする 2 行だけを足して建てた
  ものは `call_kwargs` -0.7%／-0.5%、`bm_ao_render` -1.1%／-1.2%（動かない）。

呼ばれない関数の中身で数 % 動くのは、`codegen-units = 1` で crate 全体が 1 つの単位になり、どこを
変えても命令ループの配置がずれるため、と読んだ（`2026-09-17-release-prep.md` の配置の話と同じ
種類）。原因の関数を特定する手は、この PC（WSL2）では `perf stat` のハードウェアカウンタが使えず
（`The instructions:u event is not supported.`）無い。`ary_aset` の後半を `#[inline(never)]` の関数に
出す案も試したが +5.1%／+3.6% で変わらず、戻した。ここで追うのを止めた（計画の外の数 % を、原因の
分からないまま深追いしない）。

ほかに、意味を変えない 2 つも試した: (v7) 外へたどるときに同じクラスを 2 度引かない（メソッドの
proc とそれを囲むクラス本体の proc は同じクラスを名指す）、(v8) `OP_GETMCNST` の外れの
`const_missing` を `#[cold]` の関数に出す。どちらも `call_kwargs` +4.8%／+4.7%、+7.2%／+7.8% で
良くならず、v7・v8・`ary_aset` の分割を全部入れた形の 2 巡は全体 +1.5%／+0.8%（2 巡目は
`bm_so_lists` が base 側で +31.9% になる遅い状態に当たった）、`call_kwargs` +7.5%／+7.4%、
`loop_while_add` +8.3%／+7.4% と v6 より悪かった。3 つとも戻し、v6 に戻した（建て直した
バイナリの md5 が v6 のものと同じ `ecac94fc…` になることを確かめた）。

## 性能

最後の形（v6、`ecac94fc…`）と base（`3078fb4`、`3073d2a3…`）。`tools/bench_ab.sh --core 2`、5 回、
向きを入れ替えて 2 巡。A/A は同じ日の base 同士 2 回。2 巡目は「base が何 % 速いか」で出るので、
v6 の増分に直した値（括弧の外）を並べる。

| benchmark | A/A 1 | A/A 2 | 1 巡目 | 2 巡目 |
|---|---:|---:|---:|---:|
| app_json_hash | +0.8% | -0.1% | +3.4% | +2.4% |
| app_robot | +0.3% | +0.4% | -0.8% | +0.2% |
| app_tak | +1.4% | +0.2% | +0.9% | +0.8% |
| bm_ao_render | +1.3% | +1.1% | +3.1% | +3.2% |
| bm_fib | +0.1% | +0.1% | +0.4% | +0.2% |
| bm_mandel_term | -1.9% | -0.9% | +2.4% | +0.1% |
| bm_so_lists | +3.5% | -0.3% | -0.2% | -1.2% |
| bm_so_mandelbrot | -0.0% | -0.6% | +0.9% | +0.5% |
| call_args | +2.2% | -1.7% | +0.8% | +0.2% |
| call_block_yield | +2.6% | +1.1% | -0.5% | -0.7% |
| call_fiber | -0.1% | +0.1% | +0.1% | -0.4% |
| call_kwargs | +0.8% | -0.8% | +5.4% | +4.6% |
| ds_array | -0.2% | +0.6% | -1.4% | -1.3% |
| ds_hash | -0.5% | +0.6% | -0.1% | +0.5% |
| ds_string | -0.3% | +0.3% | -2.1% | -1.0% |
| gc_churn | +0.4% | -0.8% | -0.7% | +0.1% |
| loop_if_branch | -0.7% | -1.1% | -1.2% | +0.9% |
| loop_times | -0.4% | -0.0% | -0.0% | -1.0% |
| loop_while_add | +1.3% | +1.2% | -0.4% | -0.5% |
| mem_retained | -1.8% | -0.2% | -4.8% | -3.7% |
| mem_short_lived | -0.6% | +0.1% | -4.2% | -3.6% |
| vm_optimization_bench | -2.3% | +0.4% | -1.4% | -2.0% |
| vmo_arith | +0.6% | -0.1% | -2.1% | -3.0% |
| vmo_calls | -0.5% | -0.1% | +0.0% | -1.5% |
| vmo_dispatch | +0.2% | -0.1% | -0.6% | -1.4% |
| vmo_index | +0.1% | -0.3% | -0.4% | -0.7% |
| vmo_objects | +2.3% | -1.7% | -1.0% | -1.6% |
| **all** | -0.1% | +0.2% | -0.2% | -0.5% |

全体と、定数を引く `vmo_*`・`vm_optimization_bench` は線の内か速い側。線（A/A の 1 本ごとの
-2.3〜+3.5%）を越えて遅いのは `call_kwargs`（+5.4%／+4.6%、9 回ずつの取り直しで +5.3%／+3.5%）、
`bm_ao_render`（+3.1%／+3.2%、取り直し +2.7%／+2.8%）、`app_json_hash`（取り直し +2.4%／+2.7%）。
上の節のとおり、`call_kwargs` は定数を引かず、この差は配置によるものと読んでいて、原因は特定して
いない。

## L2・L4. `attr_*` は public、`method_added` なし、答えは nil

### rc2 の作り

`mrb_mod_attr_reader` などは `mod_attr_define`（`src/class.c:3114`）で、引数ごとに
`prepare_ivar_name`（`@` を付けて `mrb_iv_name_sym_check`）をし、cfunc の proc を作って
`mrb_define_method_raw` に渡す。可視性の旗は付けない（`MT_VDEFAULT`）ので、
`mrb_define_method_raw`（`src/class.c:1034`）が `find_visibility_scope(mrb, c, 0, …)` で決める。
ここで見るのは `mrb->c->ci`、つまり **`attr_reader` という cfunc 自身のフレーム**で、その proc は
cfunc（`upper` が NULL）なので `check_visibility_break` は最初で真になり、cfunc のフレームの
可視性（既定の public、modfunc なし）が使われる。だから `private` の下でも `module_function` の
下でも、別のクラスに向けた `other.attr_accessor` でも public になる。

同じ読みから、計画に無かった差が 3 つ出た（`t/b.rb`）:

- `mrb_define_method_raw` は `method_added` を呼ばない（呼ぶのは `define_method_m` などの側）。
  SabiRuby は `Vm::def_method` を通していたので `[:added, :z]`、`[:added, :z=]` が出ていた。
- 答えは `mrb_nil_value()`。SabiRuby は CRuby と同じく名前の Array を答えていた。
- 名前の検査は `mrb_iv_name_sym_check` の文言 `'@a?' is not allowed as an instance variable name`。
  SabiRuby は `invalid attribute name 'a?'` だった。

どれも `mod_attr_define` 1 つから出る答えなので、L2 と同じ関数の写しとして直した。`attr_accessor`
は rc2 では reader を全部定義してから writer を全部（`mrb_mod_attr_accessor` が 2 つを順に呼ぶ）
なので、途中の名前で例外になったときに何が定義済みかもそれに合わせた。

`private attr_reader :x`（CRuby の書き方）は、rc2 では `private(nil)` になる。rubevy・rubevy_games・
この repo の `.rb` を `grep` して、その書き方と `= attr_` は無いことを確かめた。

### 直し

`src/builtins/object.rs` の `attr` を `mod_attr_define` の写しにした。定義は新しい
`Vm::def_method_nohook`（`def_method` から `method_added` を除いたもの。凍結の検査と `initialize` を
private にする規則は残す）を `Vis::Public` で。名前の検査は `ext_metaprog.rs` の `iv_name_p`
（`instance_variable_get` が使っているもの、`mrb_iv_name_sym_p` の写し）を `pub(crate)` にして使った。
既にあった `Vm::def_method_raw` は表に書くだけで凍結を見ないので、別の名前にした。

## L3. `method_defined?`

`mrb_mod_method_defined`（`src/class.c:4135`）は `mrb_get_args(mrb, "n", …)` で引数 1 個、答えは
`mrb_obj_respond_to(mrb, c, id)`（`src/class.c:3377`）: 探して undef でも「実装なし」でもなければ
true。可視性は見ない。SabiRuby は引数 1〜2 個を受け、private を false にしていた。`argc!(vm, a, 1)` に
して、private の判定を外した（`notimpl_fns` の判定は rc2 の `MRB_METHOD_NOTIMPL_P` に当たるので残す）。

SabiRuby には `public_method_defined?`・`private_method_defined?`・`protected_method_defined?` も
あるが、rc2 には無い（`undefined method 'private_method_defined?'`）。`docs/verification/coverage.md` の
「SabiRuby にだけあるもの」に前から載っている。計画の範囲の外なので触っていない。

## L5. 文言

### 特異クラスの名前

rc2 の `mrb_vformat` の `%C`（`src/error.c:372`）は、クラスを `mrb_obj_as_string` に通す
（`L_cat_obj`）。つまり `Module#to_s` の答えで、特異クラスは `#<Class:DefInSdef>`、特異クラスの
特異クラスは `#<Class:#<Class:DefInSdef>>`、普通のオブジェクトのものは `#<Class:#<Object:0x…>>`。
SabiRuby は `Vm::class_name` を使っていて、名前の無いクラスは全部 `#<Class:0x…>` だった。
`Module#to_s`（`mod_to_s`）の中身を `object::module_str` として切り出し（特異クラスの特異クラスでも
再帰するように直した。前は内側が `class_name` で `#<Class:#<Class:0x…>>` になっていた）、NameError を
作る所でそれを使うようにした。

引用符の有無も所ごとに違った（`t/n.rb`）:

| 所 | rc2 の関数 | 文言 |
|---|---|---|
| `alias`、`alias_method` | `mrb_alias_method` → `mrb_method_search`（`src/class.c:3065`） | `for class %C`（引用符なし） |
| `private :x` など | `mrb_method_search`（`src/class.c:2523`） | 同上 |
| `module_function :x` | `mrb_method_search`（`src/class.c:4263`） | 同上 |
| `undef`、`undef_method` | `mrb_undef_method_id`（`src/class.c:3831`） | `for class '%C'`（引用符あり） |
| `instance_method`、`method` | `mruby-method/src/method.c:649` | 同上 |
| `remove_method` | `src/class.c:3954` | `method '%n' not defined in %C` |
| `const_missing` | `mrb_const_missing`（`src/class.c:4085`） | `%v::%n`、`mrb_class_real` が Object なら `%n` だけ |

SabiRuby は alias・可視性・`module_function` で引用符を付けていたので外した。`const_missing` は
「受け手が Object なら名前だけ」を `mrb_class_real` の写し（`Vm::class_real`、特異クラスと include
クラスを上へ飛ばす）で判定するようにした。

`OP_GETMCNST`（`Foo::Nope`）は、rc2 では `mrb_const_get` → `const_get` で外れると `const_missing` を
呼ぶ。SabiRuby は呼ばずに自分で NameError を作っていて、`Object::Nope` が `uninitialized constant
Object::Nope`（rc2 は `uninitialized constant Nope`）、`Bar::Nope` が `Bar.const_missing` を素通り
していた。外れたら `const_missing` を送るようにした。探す範囲（下の「残った差」）は変えていない。

### `Array#[]=` の負の添字

`mrb_ary_aset`（`src/array.c:1435`）の 3 つの道:

- 添字 1 つ: `mrb_ary_set`（`src/array.c:1110`）。`index %i out of array` に、与えた添字そのもの
  （`n += len` の後で `n - len` を書く）。SabiRuby は CRuby の `too small for array; minimum: -3`。
- 添字と長さ: `mrb_ary_splice`（`src/array.c:1171`）。長さが負なら `negative length`、次に
  `head += alen` してまだ負なら `index %i is out of array` に**足した後の** head（`a[-4, 1] = 1` は
  `index -1 is out of array`）。SabiRuby は `index out of array`。
- Range: `mrb_range_beg_len(…, trunc = FALSE)`（`src/range.c:537`）。始まりが足しても負なら
  `MRB_RANGE_OUT` で RangeError `-10..1 out of range`。`trunc` が偽なので、始まりが末尾より先なら
  OUT ではなく、nil で埋めてそこに入れる（`b[5..6] = 9` は `[1, 2, 3, nil, nil, 9]`）。SabiRuby は
  ここを `String` と共有の `index_args` に通していて、始まりが末尾より先の Range も
  `index out of array` にしていた（文言だけでなく答えも違った）。

Range の道を `ary_aset` の中に rc2 の形で書き、添字と長さの道には負の head の検査を前に足し、
添字 1 つの文言を直した。`edge_misc` の期待値にあった `too small for array` は rc2 の `out of array`
に戻した（その行は前から rc2 と違うことを断っていた）。

## テストの期待値

- `def_target_scope.expected` を `.rc.out` と同じにした（4 行が SabiRuby の答えだった。冒頭の説明も
  直した。行が変わったので `tools/custom.sh` で `.mrb` と `.dump` も作り直した）。
- `edge_misc.expected` を `.rc.out` と同じにした。
- 新しく `rc2_leftovers.rb`。`tools/custom.sh rc2_leftovers` が `.expected` を rc2 の出力から作り、
  `reference output agrees with .expected`。アドレスは `gsub(/0x\h+/, "0x")` で消した（rc2 は本物の
  ポインタ、SabiRuby は作った番号を出す）。

## 確かめたこと

- `cargo test --workspace`（debug。`const_lookup` と `Vm::const_class` の `debug_assert!` が生きた
  状態で、fixtures 18、custom 34、mrbtest 111 ファイルを回す）、`cargo test --release --workspace`、
  `cargo test -p sabiruby --no-default-features --features "std utf8"`（mruby-regexp なし）、
  `--features "std regexp"`（バイト列）: 全部通過。
- `tools/check_no_std.sh --features regexp`／`--features utf8`: `no_std OK`。最初は `array.rs` で
  `String` を prelude から使っていて no_std で落ちた（`alloc::string::String` に直した。バイナリの
  md5 は変わらない）。
- `cargo build -p sabiruby --lib --target wasm32-unknown-unknown`、`cargo doc --no-deps --workspace --lib`
  と `-p sabiruby-cli --bins`（`RUSTDOCFLAGS=-D warnings`）、`cargo check --workspace --all-targets`:
  通過。doc は一度、公開の `CallInfo::method_block` の説明から非公開の `Vm::const_class` に
  リンクしていて落ちた（リンクを外した）。
- rc2 の mrbtest（`tools/mrbtest.sh`、3 ビルド）: 2346 / 2508、2269 / 2453（バイト列）、1981 / 2012
  （regexp なし）。作業前と同じで、ファイルごとにも baseline と同じ（上がったものも下がったものも
  無い）。L1〜L5 は rc2 の `test/t` には出ない差だった。日付だけ変わった `docs/verification/mrbtest*.md`
  は戻した。
- edge_compare の再計測は下の節。

## edge_compare の再計測

コミット `e40af84`（L1〜L5）の木で `tools/edge_compare.sh`（JSON の `dirty: false`）。891d1d9d の
`test/t` 48 ファイル、mruby/edge のドライバ。

| | 前（`d74ff47`） | 今（`e40af84`） |
|---|---:|---:|
| SabiRuby: pass / fail / error | 857 / 34 / 34 | 855 / 36 / 34 |
| SabiRuby with C fixtures: pass / fail / error | 883 / 44 / 30 | 880 / 46 / 31 |
| ファイル: passed / failed / runtime_error（SabiRuby） | 29 / 18 / 1 | 29 / 18 / 1 |

下がった assert は全部、891d1d9d の master でだけ変わった挙動を確かめるもの（`../ref/mruby` で
`git log -S<assert の名前> 4e2bc5c2b..891d1d9d` で変えたコミットを探した）:

- module.rb「Module#attr_* answer the names they define」（`5e77136c3`、`attr_*` が名前を答える）:
  pass → error（rc2 の nil を答えるようになった）。
- module.rb「Module#method_defined? reports the methods a listing reports」（`77d1d928f`、private を
  false にする）: pass → fail。
- module.rb「Module#method_defined? with inherit false …」（`ff047f5fe`）: 前から通っていなかったが、
  中身が引数の数の ArgumentError に変わった。
- module.rb「Module#attr_* take the visibility of the scope …」（`2dd14996d`）: error → fail。
- module.rb「constant lookup: ancestors before the top level」（`143892a37`）と「def self.name looks up
  from the class body」（`01df203a7`）: 前から fail で、外れ方が変わった（前は継承を先に見ていた
  ので、master の答えに一部だけ合っていた）。
- syntax.rb「a private setter is callable on a written self」（with C fixtures の列だけ。SabiRuby の列は
  ファイルがその前で止まる）: pass → fail。`private` の下の `attr_writer` が rc2 では public なので、
  外から呼べてしまい「private method」の例外が出ない（`2dd14996d`、`086637afd`）。

`tests/edge-spec/notes.tsv` の module.rb と syntax.rb の行に、この 3 つの話とコミットを足して、
`tools/edge-compare/report.py` でページを作り直した。上がった assert は無い（`methods.rb` の
alias／undef の 2 つは error のままで、メッセージが `#<Class:AliasTargetInSingleton>` の形になった）。

## 気づいた点

- **VM（sabiruby）/ rc2 との差、残り**: `Foo::String`（`Foo` がクラス）と `Foo.const_get(:String)` は、
  rc2 では NameError（`const_get(..., skip = FALSE)` は Object の手前で止まる。`src/variable.c:1292`）、
  SabiRuby は Object の `String` を答える。`OP_GETMCNST` と `Module#const_get` の探す範囲の話で、
  L1〜L5 の外なので変えていない（`t/m.rb`）。同じ所で、クラスでもモジュールでもない受け手の
  TypeError の文言（rc2 `constant look-up for non class/module`、SabiRuby `not a class/module`）。
- **VM / rc2 との差、残り**: `Array#[]=` の Range の端が Float のとき rc2 は `mrb_as_int` で受ける
  （`a[(1.0..2.0)] = 1` が通る）が SabiRuby は TypeError、`a[nil] = 1` の文言（rc2
  `nil cannot be converted to Integer`、SabiRuby `no implicit conversion into Integer`）。前から。
- **VM / rc2 にだけ無いもの**: `public_method_defined?`・`private_method_defined?`・
  `protected_method_defined?` は rc2 に無い（`coverage.md` の「SabiRuby にだけあるもの」に載っている）。
- **rc2 のふるまい（本の素材）**: rc2 の `class_eval("…")`（文字列）は、与えたクラスを呼び出し元と
  共有している環境に書き込む（mruby-eval の `MRB_PROC_SET_TARGET_CLASS`）。そのため
  `def f; M.class_eval("X"); end; p f` の後、トップレベルの `def` の行き先が `M` に変わり、rc2 では
  次の行の `f` が `undefined method 'f' for Object` になる（スクラッチ `t/e.rb`）。SabiRuby は
  文字列の proc 自身に書くので、これは真似していない。
- **rc2 のふるまい（本の素材）**: include したモジュールのメソッドの本体と、その中のブロックで、
  定数の見え方が違う（ブロックからは include したクラスの祖先が見える）。フレームの class が
  「メソッドを見つけた include クラス」で、ブロックの環境がそれを写すため。rc2 の定数キャッシュ
  （irep と名前で引く 64 項目）は、同じ irep を違う字句の場所から走らせる（`class_eval` に同じ
  ブロックを渡す）と前の答えを返しうるが、確かめていない。
- **計測**: 定数を 1 つも引かない `call_kwargs` が、呼ばれない関数（`ary_aset` の後半）の中身で
  +2〜+7% 動いた。`codegen-units = 1` での配置の揺れと読んだが、この PC ではハードウェアカウンタ
  （`perf stat`）が使えず確かめられない。配置を固定する手（関数の整列、PGO）は計画の外。
- **作業の手順**: 共有の `target/` の debug のテストバイナリが古い `sabiruby` を引いていて、
  `cargo test` が直したはずの Hash の探索で落ちた（`touch` では直らず、`cargo clean -p sabiruby`
  で直った）。implementer.md の注意のとおり。
