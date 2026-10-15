# 2026-10-15 891d1d9d の本家テストで見つかった差を直す

計画: `docs/plans/edge-diff-fix-plan.md`（D1〜D6）。前段の比較（`2026-10-15-edge-compare.md`）で、
upstream `891d1d9d` の `test/t` に対して SabiRuby が落ちた assert 69 個のうち、C fixture の要らない
ものを 1 件ずつ本家 `891d1d9d`（Docker `kishima/mruby:891d1d9d`）の挙動と比べて直した。
本家のソースは `../../ref/mruby`（作業ツリーは rc2 のまま）から `git show 891d1d9d:<path>` で読んだ。

確かめ方は 2 通り。本家の挙動は `docker run kishima/mruby:891d1d9d mruby x.rb` で、SabiRuby の側は
`tools/edge_compare.sh` が作る `target/edge-compare/ref/*.mrb`（891d1d9d の `mrbc -g` でコンパイル
したテストそのもの）を `sabiruby mrbtest -v assert.mrb <file>.mrb` で走らせた。

## D1. 演算子の再定義が opcode に届かない

### 本家のやり方

891d1d9d の `src/vm.c` の `OP_MATH`（`OP_ADD`/`OP_SUB`/`OP_MUL`）、`OP_MATHI`、`OP_MATHILV`、
`vm_op_div`、`OP_CMP`、`OP_EQ` は、Integer の組を C で答える前に
`mrb->bop_redefined & MRB_BOP_INTEGER(op)` を見る。`bop_redefined` は (クラス, 演算子) の組ごとの
ビット（`include/mruby.h` の `MRB_BOP_ADD`〜`MRB_BOP_GE` の 9 個 × Integer と Float、
`Symbol#==`、nil/true/false の `==` の 1 ビット）。`src/class.c` の `bop_arm`/`bop_refresh` が、
組み込みの関数ポインタを `bop_builtin` に覚え、メソッド表が変わるたびに
（`mrb_builtin_op_update(mrb, mid)`、`include` のときは `mid = 0` で全部）、その演算子が今も
同じ関数に解決されるかでビットを立て下ろしする。Integer 以外の数の組（Integer と Float の混合、
Float 同士）は、Integer と Float のどちらかのビットが立っていれば送る（`MRB_BOP_NUMERIC`）。
割り算は `vm_op_div` の頭で、どちらかが立っていれば Integer 同士も送る。

`OP_EQ` には別の速い道がある。同じ値の 2 つ（`mrb_obj_eq`）は true、Symbol は他の何とも等しくない。
これを、Integer/Float/Symbol/nil・true・false のどれかの `==` が置き換わっている間は飛ばす。さらに
ヒープのオブジェクトは、クラスに `MRB_FL_CLASS_EQ_DEFINED` が付いていれば同一でも `==` を送る。
この印は `mrb_define_method_raw` で `==` が定義されたとき（bootstrapping 中を除く）に付き、
サブクラス・特異クラス・include 先に伝わる（`eq_defined_mark`）。bootstrapping 中に読まれる core の
mrblib（`Comparable#==` は Ruby で書かれている）は印を付けない。

`mrb_equal`（`src/object.c` の `mrb_equal_in_c`）も同じビットを読む: Integer 同士・Symbol 同士は
組み込みのままなら値で答え、そうでなければ `==` を送る。`Array#index` などがこれを通る。

### SabiRuby に入れたもの

SabiRuby には同じ形の仕組みが既にあった（`[]`/`[]=` の `idx_builtin`/`idx_class`、
`src/vm.rs` の `idx_sync`）。違いは、本家がメソッド表の変更のたびに能動的に再計算するのに対し、
こちらは `Heap::method_serial`（メソッドキャッシュを捨てる世代番号、`Heap::class_mut` で上がる）が
変わったことを次の opcode で見て、遅れて再計算すること。`bop_*` も同じ作りにした。

- `Vm::bop_builtin`（22 スロット: Integer 9、Float 9、Symbol 1、nil/true/false 3）、
  `bop_redefined`（ビット）、`bop_serial`。nil/true/false は本家ではクラスの印を
  1 ビットに写すが、ここでは 3 つのスロットが 1 ビットを共有する形にして、他と同じく「今解決される
  ものが組み込みか」で決める。違いは下の「本家と違う点」。
- `bop_init` は `idx_op_init` の直後（`builtins::init` の最後）。本家の `mrb_builtin_op_init` も
  core の初期化の直後で、gem の初期化より前。
- 速い道の検査は **bool を 1 つ読むだけ**にした（`Heap::ops_clean`）。`Heap::class_mut` と
  クラスの割り当て、つまり `method_serial` を上げる 2 か所で false にし、`Vm::bop_sync` が
  「1 ビットも立っていない」ときに true に戻す。`OP_ADD` などはこれが true のときだけ従来どおり
  2 つの数をその場で計算し、それ以外は `op_arith`/`op_compare` に行って、そこで（必要なら再計算して）
  その演算子のビットを読む。本家の 1 回のマスク検査と同じ重さを狙った。メソッドを定義した直後の
  最初の算術で 22 回のメソッド探索が走るのは、`idx_sync`（6 回）と同じ種類の費用。

  最初は `Vm` に `bop_clean: u64`（ビットが全部下りていたときの世代番号）を持たせ、
  `self.bop_clean == self.heap.method_serial` を比べていた。下の性能の節にあるとおり、これで
  `bm_mandel_term`（18 ms の算術のループ）が 7% 遅くなり、検査を外した版と比べると半分ほどが
  この比較だった（u64 を 2 つ読む）。bool 1 つにして、加えて `OP_DIV` にも Integer 同士の速い道を
  置いた（それまで `OP_DIV` は毎回 `op_arith` を呼んでいた。本家の `vm_op_div` も関数だが、
  呼び出しの前にビットを 1 回見るだけ）。
- `op_arith` は計算部分を `arith_value` に分けた（`None` = 送る）。`OP_ADDILV`/`OP_SUBILV` は
  ローカル変数のレジスタの上に送りを組めないので、本家と同じくその場で `funcall` する。前の
  コードは `op_arith` を作業レジスタで呼んで結果を写していたが、送った場合はフレームが積まれる
  だけで、写す値がまだ無い。
- `op_compare` の `OP_EQ` の同一性の答えは、ヒープのオブジェクトなら `==` を引いて、ネイティブか、
  core の mrblib が書いた `==`（`Vm::eq_core`、`load_mrblib` で core の mrblib を読んだ直後に集める。
  今は `Comparable#==` の 1 つ）のときだけ true にする。本家のクラスの印を、解決結果で読み替えた
  もの。`eq_core` の Proc は GC の根にした（解放されて番号が別の Proc に使い回されると誤る）。
- `Vm::equal`（`mrb_equal` に当たる）は、Integer 同士・Float 同士・Symbol 同士で同一でないとき、
  ビットが立っていれば `==` を送る。本家は Float 同士を常に送るが、組み込みの `Float#==` の答えは
  値の比較なので、組み込みのままなら値で答えた。

確かめたこと: `integer.rb` の 3 つ、`float.rb` の 2 つ、`symbol.rb`、`basicobject.rb` の 2 つが
通った。手で書いた比較（`tests/custom/operator_redefinition.rb`）は 891d1d9d の出力と一致する。
`6 / 3` は `Float#/` だけ置き換えた間、本家は Integer 同士も送るが、送った先が組み込みの
`Integer#/` なので答えは 2 で、こちらも同じ。

### 本家と違う点（残したもの）

nil/true/false の `==` の印は、本家ではクラスの印なので一度付くと消えない。`NilClass#==` を定義して
alias で戻した後も、`OP_EQ` はすべての受け手で同一性の答えを飛ばして `==` を送る。その結果、
本家ではその後 `Comparable` を include しただけのオブジェクトで `cmp == cmp` が false になる
（`Comparable#==` は `<=>` が nil だと false。手で確かめた）。SabiRuby は解決結果で読むので、
戻せば同一性の答えに戻り true。テストの注釈（「組み込みの `==` は同一性の答えを保つ、Comparable の
ものも」）はこちらの答えを意図しているので、本家の印の残り方には合わせなかった。custom テストでは
この並びを最後に置き、本家と同じ出力になるようにした。

### rc2 の mrbtest で 1 つ落ちたもの

`gem_enum`（rc2 の `mrbgems/mruby-enum-ext/test/enum.rb`）「Enumerable#count - an argument decides
over a block」の最後の assert（`==` が常に false のオブジェクトを `count` が 1 と数える）が落ちる。
rc2 の `Enumerable#count`（mrblib、Ruby）は `val.__svalue == v` で比べ、rc2 の注釈も「`==` reaching
it through OP_EQ」、つまり rc2 の `OP_EQ` が同一なら `==` を送らないことに頼っている。891d1d9d は
`OP_EQ` のこの答えをやめ、同じ assert を通すために mrblib の方を `val.__svalue_eq(v)`（C の
`mrb_equal` を呼ぶ `Array#__svalue_eq`、`src/array.c`）に書き換えた（注釈も「as `rb_equal()` takes
it」に変わっている）。SabiRuby の gem の mrblib は rc2 のものなので、891d1d9d の VM の挙動と rc2 の
mrblib の組み合わせではこの assert は落ちる（891d1d9d の本家に rc2 の mrblib を載せても同じはず）。
計画の「rc2 と 891d1d9d で挙動が違う場合は 891d1d9d に合わせる」に従い、`gem_enum` の基準を
38 から 37 に下げた。mrblib を 891d1d9d のものにするのは、gem の Ruby 部分を別のバージョンに混ぜる
ことになるので、この計画ではしていない（著者の判断に残す）。

## D2. `$!` と引数なしの `raise`

比較の前段では「rescue 修飾子と ensure の中で `$!` が nil」と見ていたが、違った。891d1d9d の
`mrbc` は `$!` を opcode で扱う: rescue 節の頭で `GETGV R, $!` で前の値を取っておき、
`SETGV $!, 例外` で名前を付け、出口で `SETGV $!, 前の値` で戻す（`mrbc -v` で確かめた。ensure も
同じ形）。SabiRuby の `GETGV`/`SETGV` は普通のグローバル変数なので、891d1d9d でコンパイルした
コードではそのまま正しく動いていた（`$!.message` の assert は通っている）。落ちていたのは、
`$!` を読む assert ではなく、その中の `raise('a') rescue raise` のような **引数なしの `raise`** だった。

891d1d9d の `src/kernel.c` `mrb_f_raise` は引数 0 のとき `$!` を読み、nil でなければそれを投げ直し、
nil なら空のメッセージの RuntimeError。rc2 は常に RuntimeError（rc2 の `mrbc` は `$!` を設定しない
ので、`$!` を手で入れても rc2 の `raise` は投げ直さない。手で確かめた）。`builtins/kernel.rs` の
`raise` を 891d1d9d に合わせた。rc2 の `mrbc` のコードでは `$!` が nil のままなので、rc2 の
mrbtest の結果は変わらない。

`exception.rb` の 3 つが通った。`tests/custom/bare_raise.rb`（`$!` を手で入れて `raise`）の期待値は
891d1d9d の出力で、rc2 の出力（`.rc.out`）とは違う。

## D3. `defined?` の定数パスとレシーバ付きメソッド、15 個の位置引数とキーワード

### `defined?`

891d1d9d の `mrbc -v` を見ると、`defined?(A::B::C)` は
`LOADSELF; LOADNIL; LOADSYM :A; LOADSYM :B; LOADSYM :C; ARRAY; SSEND :__defined_const_path? n=2`、
`defined?(::A::B)` は根に `OCLASS` を置いた同じ形、`defined?(o.foo)` は
`SSEND :__defined_method_on? n=2`（受け手と名前）になっている。rc2 の `mrbc`（SabiRuby の `eval` が
使う `compiler/vendor` の写しも同じ）は `A::B` の形だけを `__defined_const_path?(:A, :B)` で問い、
`__defined_method_on?` は無い。SabiRuby の `__defined_const_path?` は rc2 の 2 つの Symbol の形しか
受けていなかったので、`nil is not a symbol nor a string` になっていた。

本家の `src/kernel.c` の `mrb_f_defined_const_path` と `mrb_f_defined_method_on` を写した。
`__defined_const_path?` は 2 番目の引数が Array なら 891d1d9d の形、そうでなければ rc2 の形として
両方受ける（`eval` の中の `defined?(A::B)` は rc2 の形で来る）。名前ごとの探索は
`mrb_const_get_noraise` = `const_get_nohook(mrb, c, sym, FALSE)` で、祖先を Object の手前まで見る
（`String::String` は nil）。`const_missing` は呼ばない。これを `Vm::const_get_noraise` にした。
`__defined_method_on?` は `OP_SEND` と同じ順の可視性の検査（private は nil、protected は呼び手の
`self` がその定義のクラスの仲間のときだけ）、見つからなければ組み込みでない `respond_to_missing?`
にだけ聞く。

同じ流れで `__defined_method?`（`defined?(meth)`、レシーバ無し）も 891d1d9d の
`mrb_f_defined_method` に合わせた: 組み込みの `respond_to?` なら `respond_to?(:meth, true)` の答え、
再定義されていればそれを呼ぶ。`respond_to?` 自体の可視性の扱い（D4 の kernel.rb の 3 つ）もここで
入れた（`builtins/object.rs` の `respond_to_p`、本家の `obj_respond_to_p`）。rc2 の `respond_to?` は
見つかったメソッドなら可視性によらず true で、`tests/custom/visibility_respond_to` の期待値は rc2 の
答えだった。891d1d9d の答えに書き換え、ファイルの頭に理由を書いた。

### 15 個の位置引数とキーワード

`def m15(p1, ..., p15, k: 1)` に `k: 7` が届かない。rc2 でも同じで、891d1d9d で直った
（`9f3a1ccb9 vm.c: deliver the keywords of a method with 15 or more parameters`）。
バイトコードは rc2 と 891d1d9d で同じ。`OP_ENTER` の後の `ci.n` は位置引数の数だが、15 は
「引数が 1 つの Array に詰めてある」の印と同じ値なので、`OP_KARG`/`OP_KEY_P`/`OP_KEYEND` が
キーワードの Hash を R2 に探していた。本家は `ci->n` を 15 で飽和させ、15 以上なら `OP_ENTER` の
オペランドから位置を読み直す（`enter_kwpos`）。SabiRuby の `ci.n` は u8 で 16 以上は正確なので、
壊れていたのは 15 ちょうどだけだったが、本家と同じ読み方の `Vm::enter_kwpos` にして、使われなくなった
`kidx_at` は消した。

## D4. メソッドを置く先、定数とクラス変数の探索、可視性

### 本家で何が変わったか

rc2 から 891d1d9d の間に、`src/proc.c`・`src/vm.c`・`src/variable.c` に「cref」の作り直しが
入っている（`e1005cc5f`、`7bb8f3836`、`8da00e9ba`、`e53052a77`、`3d0ec2771`、`41a0af60a`、
`1c6ee990c`、`7cbf44f21`、`9b1425f85`、`01df203a7`、`143892a37`）。要点:

- クラス本体・メソッド本体・クラスを与えられた eval 文字列の Proc に `MRB_PROC_CREF` を付け、
  その Proc の target class を「書かれた場所のクラス」として持たせる。`def self.go` の本体の Proc は、
  以前は入れ先の特異クラスで上書きされていたが、外側のクラスを持ったままにする。
- `mrb_vm_cref_class(ci)`: Proc の `upper` を CREF（GIVEN でない）まで辿った target class。
  定数の探索と定義（`OP_GETCONST`、`OP_SETCONST`、`OP_CLASS`/`OP_MODULE` の外側）はこれを使う。
- `mrb_vm_definee_class(ci)`: `def`/`alias`/`undef`/`OP_TCLASS` の入れ先。フレームがクラスを
  与えられていれば（`class_eval`・`instance_eval`・その `_exec`・`Class.new`/`Module.new` のブロック、
  `MRB_CI_GIVEN_CLASS`）そのクラス、そうでなければ Proc を辿って CREF の target class、途中の env に
  `MRB_ENV_GIVEN_CLASS` があればその env のクラス。
- 与えられたクラスのブロックの中で書かれたメソッドは、Proc に与えられたクラスを持ち
  `MRB_PROC_GIVEN` を付ける（`mrb_method_proc_new`）。定数とクラス変数の探索はこの Proc を飛ばして、
  ブロックが書かれた場所から読む。
- 定数の探索（`mrb_vm_const_get`）: cref 自身の表、外側の字句スコープの表（トップレベルは除く、
  GIVEN は除く）、cref が特異クラスなら特異クラスの祖先を先に、そのあと cref の祖先、モジュールなら
  Object。クラス変数は `cv_scope_class`（CREF で GIVEN でない、特異クラスでない最初の Proc）。

### SabiRuby に入れたもの

SabiRuby の作りは rc2 の本家に近かった。ブロックの Proc の target class はフレームの target class、
メソッドのフレームの `target_class` は見つかったクラス（`super` が使う）、`def`/`alias`/`undef`/
`TCLASS` はフレームの `target_class`、定数の探索は「フレームのクラスとその祖先、`upper` の各 Proc の
クラスとその祖先、Object」という独自の順だった。

これを本家の作りに写した。

- `ProcData` に `cref` と `given`、`EnvData` に `given_class`、`CallInfo` に `given_class` を足した。
  与えられたクラスのフレームは `exec_block_with_self`/`call_block_with_self_kw` だけが作るので、そこで
  `pending_given` を立て、`call_proc_inner`/`push_frame_at` がフレームに移す（`pending_vis_break` と
  同じやり方）。`vis_break` を使い回さなかったのは、`run_eval`（`eval` の文字列）も `vis_break` を
  立てるが、本家では eval の文字列のフレームは「与えられた」ではないため。
- env を作るとき（`frame_env_at`、`frame_env_at_ctx`）に、本家の `mrb_env_new` と同じ条件で
  `given_class` を決める（`Vm::env_given_class`）。Binding の env はもとの env の印を写す。
- `Vm::cref_class`、`Vm::definee_class`、`check_target_class`、`method_proc_new`（`TDEF`/`SDEF`/
  `OP_METHOD`）、`lexical_scope_p`、`const_walk`/`const_get_skip`（`mrb_vm_const_get` と
  `const_get_nohook(skip=TRUE)`）、`cvar_class`（`cv_scope_class`）。`OP_EXEC` の Proc は `cref`。
  `instance_eval`/`class_eval` の文字列の Proc は target class を与えられたクラスにして `cref`。
- ブロック（`OP_BLOCK`/`OP_LAMBDA`）の Proc の target class はフレームの `target_class` のまま。
  本家でも env を持つ Proc の `MRB_PROC_TARGET_CLASS` は env の `c`（フレームのクラス）なので同じ。

`TDEF` の可視性: 以前は入れ先が特異クラスなら常に public にしていた。891d1d9d は `class << self` の
本体の `private` が下の `def` に届く（`33163ec59`）ので、`def self.x`（`SDEF`）だけを public にした。

`attr_*` と `define_method` の可視性（`2dd14996d`、`94aa382c3`、`5e77136c3`）: 本家の
`caller_scope_visibility` を `builtins/object.rs` に写した。呼び手のフレームのクラスと `self` が
そのクラス自身のときだけスコープの可視性を使い、他のクラスに向けた呼び出しやメソッドの中からの
呼び出しは public。`module_function` のスコープでは `attr_*` は private で特異クラスに写さず、
`define_method` は private にして特異クラスに public の写しを作る。rc2 の `define_method` は常に
public で、`tests/custom/visibility_toplevel_def` の 1 行がその答えだった。891d1d9d の答えに書き換えた。

`method_defined?(name, false)`（`ff047f5fe`）: 自分の表（prepend があれば origin）だけを見る。
undef と未実装は false、private は false。

確かめたこと: `methods.rb`、`class.rb`、`module.rb`、`kernel.rb`（C fixture の 1 つを除く）が
参考列で全部通る。rc2 の mrbtest は 1 つも変わらなかった（gem_enum の 1 つは D1 による）。
`tests/custom/def_target_scope.rb` は 891d1d9d の出力と一致し、rc2 の出力（`.rc.out`）とは
1 行目から違う（rc2 は `def self.ali; alias aliased orig; end` で NameError）。

## D6. その他の 4 つ

- `Integer#%`: 891d1d9d の `80747a5f6` が `int_mod` の「受け手が 0 なら 0 を返す」を消し、
  `0 % 0` と `0 % 0.0` が ZeroDivisionError、`0 % 2.5` が Float になった。SabiRuby の Integer 同士は
  もともと raise していたが、Float の除数 0 は NaN を返していた（`p % q` のまま）。本家の `flo_mod` と
  `int_mod` の Float の枝は rc2 から `flodivmod` で、除数 0 で raise する。rc2 でも同じだったので
  SabiRuby だけの差だった。除数 0 のときだけ raise するようにした（剰余の計算は今のまま。fmod に
  符号の補正をする点は `flodivmod` と同じ答え）。
- `Array#[]` の Float の添字（`e2148c1b1`）: `aget_index` が `mrb_as_int` を使うようになり、
  Infinity・NaN は RangeError「float Infinity out of range」、`1e30` は「integer out of range」。
  `Vm::expect_int` は FloatDomainError と別の文言を出すので使わず、`builtins/array.rs` に本家と同じ
  文言の変換を書いた（`[]` と `[]=` の添字と長さ）。
- 式展開（`OP_STRCAT`）: 891d1d9d は組み込みの値（Integer、Float、Symbol、nil、true、false、
  BigInt）を送らずに綴り（`mrb_obj_as_string_nomethod`）、それ以外は `to_s` を送って、String で
  ない答えなら既定の表現（`mrb_any_to_s`、`prepare_exec_strcat_post_func`）にする。SabiRuby は
  文字列以外を全部 `to_s` で送り、String でない答えを空文字列にしていた。`Vm::strcat_string` にした。
  `as_string`（join などが使う）は変えていない。
- `GC.step_ratio=`/`GC.interval_ratio=`: 本家は C の `int` に入れるので、`INT_MAX` を超える値と
  負の値を ArgumentError にする（文言は本家どおり）。`i32::MAX` は本家の境界で、SabiRuby の
  既定値ではない（コメントに書いた）。

## 参考列だけで見えた差（syntax.rb の後半）

mruby/edge の driver では `syntax.rb` がトップレベルの `TestNotImplement` で止まるので、それより
後の assert は数えられない。本家の `assert.rb`（参考列）では最後まで走り、D3 を直したあとに
次の 4 つが新しく見えた。どれも 891d1d9d で入ったテストで、同じ種類の差なので直した。

- `defined? answers with a frozen string`: `__defined_*?` の答えを凍結した文字列にした
  （`033109642`）。"expression" や "local-variable" のようにコンパイラがリテラルで書く答えは、
  891d1d9d の `mrbc` が `STRING; SEND :freeze` を出す。
- `a deconstruction hook has to answer a Hash`: 891d1d9d の `mrbc` はハッシュパターンで
  `deconstruct_keys` の答えに `__pat_values` を送る。Hash 以外のための `Kernel#__pat_values`
  （TypeError「deconstruct_keys must return Hash」、`obj_pat_values`）を足した。
- `an operator instruction checks visibility on the send it falls back to` と
  `super reaches a private or protected operator`: `OP_ADD`・`OP_EQ`・`OP_GETIDX` などが送りに
  落ちるとき、受け手が呼び手の `self` でなければ可視性を検査する（`90e568e8e`）。
  `Vm::op_send_operator`。

`tests/custom/operator_visibility.rb`。

## D5. Hash の `eql?` が Hash を変える場合 — 直していない（著者の判断待ち）

4 つのテストを本家で確かめると、どれも「キーと hash code の違うエントリにも `eql?` を聞く」ことが
前提になっていた。

```ruby
k = Class.new { def eql?(o) $calls << o; false end; def hash; 42 end }
h = {"a" => 1, "b" => 2, 3 => 3}; h[k.new]          # 本家 ["a", "b", 3]、SabiRuby []
t = {}; 20.times { |i| t[i] = i }; t[k.new]          # 本家 [9, 4, 17]、SabiRuby []
```

本家（rc2 も 891d1d9d も）の Hash はエントリに hash code を持たず、小さいもの（AR）は全エントリに
`eql?` を聞き、大きいもの（HT）は index bucket の探索列に並んだエントリに聞く。SabiRuby は CRuby と
同じく hash code が一致したエントリにだけ聞く（`HashData` の `hashes` と `index`）。テストの `eql?`
はそこで Hash を変え、本家は `H_CHECK_MODIFIED`（容量・ポインタ・件数）と `entry_check_vacated`
（一致したエントリが同じキーのままか）で「hash modified」を投げる。

SabiRuby には件数の検査はもうある（`hash_index_at`、rc2 の時点で入れたもの）。テスト 1 と 2 は
`eql?` が呼ばれないので、この検査に届かない。テスト 3 と 4（`assoc`・`rassoc`・`==`・`eql?`・
`inspect`・`**rest`・`rehash`）は hash code が揃っている（42）ので `eql?` は呼ばれるが、SabiRuby の
これらの実装はエントリの写しを先に取って回る作りで、本家のように「回っている途中のエントリが
空けられたか」を見る場所が無い。

案:

- **A. 今のまま（CRuby の順）。** 4 つは落ちたまま。SabiRuby はメモリ安全なので、本家がこの検査で
  防いでいる範囲外の読み出しや解放後の使用は起きない。差は「`eql?` が何回、どのエントリに対して
  呼ばれるか」と「途中で変えたときに例外になるか、写しから答えるか」。`docs/verification/
  mrbtest-notes.md` の「意図した差」に hash.rb の 4 つとして書く。
- **B. テスト 3 と 4 だけ合わせる。** 各スキャン（7 か所）で、コールバックの後にエントリがまだ同じ
  位置に同じキーで残っているかを見て、違えば RuntimeError「hash modified」。`HashData` は変えない。
  2 つの assert（3 と 4）が通る見込み。テスト 1 と 2 は A と同じ。
- **C. 本家の Hash の探索順を写す。** 小さい Hash では全エントリに `eql?` を聞く（hash code を見ない）。
  大きい Hash で本家と同じエントリに聞くには、本家の index bucket の作り（二次探索の順）まで写す
  必要がある。`HashData` の設計が変わり、`eql?` の呼ばれ方が CRuby と離れ、探索の速さにも響く
  （小さい Hash でも、組み込みでないキーのたびに Ruby の `eql?` が走る）。

どれにするかは著者に聞く。B は設計を変えないが、本家のメモリの事情から来た挙動を、それの無い
SabiRuby に持ち込むことになる。

前の判断: rc2 で 1 つ目のテストが入ったとき（2026-09-23、`docs/worklog/2026-09-23-rc2.md` の
(a)/(b)、`docs/plans/release-0.7-plan.md` S1）、著者は (a)「`eql?` が呼ばれる場合だけ件数の変化を
検査して raise する」を選び、(b)「hash code の違うキーにも `eql?` を聞く」は「Hash の大きさで
探索の意味が変わる」という理由で採らなかった（`docs/verification/mrbtest-notes.md` の hash の行）。
上の A がその続き、C がそのとき採らなかった (b) にあたる。891d1d9d の 3 つの新しいテストも同じ
前提（違う hash code のエントリに `eql?` を聞く）に立っているので、判断の材料は前と同じで、
テストの数が 1 から 4 に増えた。
