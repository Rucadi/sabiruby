# 2026-10-16 本家 master への追従の計画と、4.1.0-rc2 に戻した記録

計画: `docs/plans/follow-master-plan.md`（M1〜M5）。作業中に計画が二度変わった。

1. 着手時の基準: 本家 master の先頭 `158760853`（`git fetch` 2026-10-16。
   `158760853c8b4f58a9913912536d012a092e3b6d`）。`tools/edge-compare/build-image.sh 158760853…` で
   Docker イメージ `kishima/mruby:15876085` を作った。ソースは `../../ref/mruby` の作業ツリーを
   変えずに `git archive` で `../../ref/mruby-158760853` に出した。
2. 本体からの変更（同日）: 本家の 4.1.0-rc2 は `stable` ブランチにあり、master の祖先ではない
   （分岐点 `4e2bc5c2b`、2026-09-04。`git merge-base 4.1.0-rc2 158760853` で確かめた。分岐の後に
   stable に入ったのは 9 コミットで、master からの取り込みは無い）。M3（mrblib とテストを master に）
   と M4（コンパイラを master に）は止めた。どちらも着手前だったので戻すものは無い。
3. 著者の決定（同日）: **SabiRuby は 4.1（4.1.0-rc2、stable）の挙動を守る。** edge-diff-fix で
   891d1d9d（master の途中）に合わせた直しのうち、rc2 と 891d1d9d で本家の挙動が違うものは rc2 に
   戻す。同じものは残す。

この記録は、その順に: rc2 と 891d1d9d の比べ方と表、戻したもの、M1（Hash）、M2（性能）、
M5（再計測とページの列の追加）。

## rc2 と 891d1d9d の挙動を 1 件ずつ比べる

### 比べ方

2 通りで確かめた。

- **手で書いた断片**（スクラッチの `probe/p.rb`）: edge-diff-fix の D1〜D6 と計画外の 3 つの
  それぞれを数行の Ruby にし、`kishima/mruby:4.1.0-rc2` と `kishima/mruby:891d1d9d` の `mruby` で
  走らせて出力を並べた。どちらもそのイメージのコンパイラでソースから走るので、「本家の挙動」は
  コンパイラ込みの答えになる。
- **891d1d9d の `test/t` 全部を assert ごとに**: `tests/edge-spec/upstream/t` の 48 ファイルを、
  `assert.rb` と `t_print` の 1 行の定義を前に付けて両方のイメージの `mruby` で走らせ、assert ごとの
  結果を表にした。同じファイルを SabiRuby で、891d1d9d の `mrbc` のバイトコード（edge_compare が
  使うもの）と rc2 の `mrbc` のバイトコードの両方で走らせ、4 列で比べた（スクラッチの
  `cmp/tools/table.py`）。C fixture を呼ぶところで本家の `mruby` コマンドは止まるので、
  `syntax.rb` の後半などはこの方法では数えられない。そこは断片で確かめた。

master のどのコミットで変わったかは `git log -S<識別子> 4e2bc5c2b..891d1d9d` と件名で探した。
どれも `4e2bc5c2b..4.1.0-rc2`（stable 側）には無い。

### 表

| # | 項目 | rc2 | 891d1d9d | 変えたコミット（master） | 扱い |
|---|---|---|---|---|---|
| D1 | `Integer#+` などを再定義したとき算術・比較の命令が送るか | 送らない（`7 + 2` は 9） | 送る | `d8873e7b1`、`35b0428c9`、`fa3dead08` | 戻した |
| D1 | `obj == obj`（同じオブジェクト）でクラスの `==` を聞くか | 聞かない（true） | 聞く | `55ae46dcf` | 戻した |
| D1 | `Symbol#==` を再定義したときの `OP_EQ` | 送らない（Symbol は他と等しくない） | 送る | 同上 | 戻した（rc2 の「Symbol の受け手は false」を足した） |
| D1 | `Array#index` などの `mrb_equal` が再定義された `==` を送るか | 送る（`mrb_obj_equal_m` でない `==` は全部送る） | 送る | — | **同じ**。SabiRuby は前から送っていなかったので、rc2 の形で入れた |
| D1 | `gem_enum`「Enumerable#count - an argument decides over a block」 | rc2 の mrblib と rc2 の VM で通る | mrblib を `__svalue_eq` に書き換えて通す（`305d1f114`） | `305d1f114`（mrblib） | 戻したら通った（38 に戻った） |
| D2 | 引数なしの `raise` が `$!` を投げ直すか | 投げ直さない（RuntimeError） | 投げ直す | `c05a266a5`（`$!` を名付けるコンパイラは `b7df80fc4`、`f3542a6b9`） | 戻した |
| D3 | `defined?(A::B::C)`、`defined?(::A::B)` | nil（rc2 の `mrbc` は 2 段の形しか問わない） | "constant" | `97ef20933`、`c04b53473` | 戻した |
| D3 | `defined?(String::String)` | "constant" | nil | `c04b53473` | 戻した |
| D3 | `defined?(o.meth)` | nil | "method" など | `9b44d5d56`、`3c6c4d53a` | 戻した |
| D3 | 位置引数がちょうど 15 個のメソッドのキーワード | 届かない（`[15, 1]`） | 届く | `9f3a1ccb9` | 戻した |
| 外 | `defined?` の答えが凍結されているか | されていない | されている | `033109642` | 戻した |
| D4 | `def self.x` の中の `def`/`alias`/`undef` の行き先 | 特異クラス（`alias` は NameError） | 書かれたクラス | `e1005cc5f`、`7bb8f3836`、`8da00e9ba` ほか cref の一連 | 戻した |
| D4 | `class << self` の中の `private` が下の `def` に届くか | 届かない | 届く | `33163ec59` | 戻した |
| D4 | `private` の下の `attr_*`／`define_method` | public | private | `2dd14996d`、`94aa382c3`、`5e77136c3` | 戻した |
| D4 | `method_defined?(name, false)` | ArgumentError（引数 1 個） | 自分の表だけ | `ff047f5fe`、`77d1d928f` | 戻した（下の「前からの差」） |
| D4 | `respond_to?` と可視性、`respond_to_missing?` | 見つかれば true | private は false | `499577695`、`49c174a7c`、`fcc9ff08d` | 戻した |
| D4 | `class_eval` に渡したブロックの中の定数 | ブロックの字句スコープ（NameError） | 同じ | — | **同じ**。下の「前からの差」 |
| D4 | `Class.new { }` の中の `def` | そのクラス | 同じ | — | 同じ（もともと通っていた） |
| D6 | `0 % 0`、`0 % 0.0`、`0 % 2.5` | 0、0、0（`int_mod` が受け手 0 を先に答える） | ZeroDivisionError、ZeroDivisionError、0.0 | `80747a5f6` | 戻した。SabiRuby は前から `0 % 0` を raise していたので rc2 の形に |
| D6 | `1 % 0.0`、`1.5 % 0` | ZeroDivisionError（`flodivmod`） | 同じ | — | **同じ**。残した |
| D6 | `Array#[]` に NaN・Infinity・`1e30` の Float | nil（C のキャストで最小の Integer） | RangeError | `e2148c1b1` | 戻した。SabiRuby は前は NaN を 0 と読んでいたので、rc2 のキャストに |
| D6 | 式展開と `to_s` | `mrb_obj_as_string`: String・Symbol・fixnum・クラスは送らず、他は `to_s`、String でなければ `#<…>` | Float・nil・true・false・BigInt も送らない | `ee3cdfdf7`（と `c8443d8d0` ほか OP_STRCAT の一連） | rc2 の形に |
| D6 | `GC.step_ratio=`・`interval_ratio=` の範囲 | 何でも受ける | C の int を超えると ArgumentError | `e2b4585e4` | 戻した |
| 外 | 演算子の命令が送りに落ちたときの可視性 | 検査しない | 呼び手の self 以外は検査 | `90e568e8e` | 戻した |
| 外 | `deconstruct_keys` が Hash 以外を返したとき | NoMethodError（`__pat_values` が無い） | TypeError | `b2d2cd15e` | 戻した |
| 外 | `super` から private の演算子 | 届く | 届く | — | 同じ |
| D5 | Hash の `eql?` の呼び順（探索順） | AR は全エントリ、HT は探索列 | 同じ | — | **同じ**。M1 で写した |
| D5 | `eql?` が一致したエントリを空けたとき | 検査しない（解放後の読み出しの余地） | RuntimeError | `a54353ecf`、`08a0432d1`、`eb7693857` | rc2 の答え（例外にしない）。M1 の節 |

断片の出力（rc2 / 891d1d9d / 戻した後の SabiRuby）はスクラッチに残したが、要点は上の表と、
`tests/custom/` の 6 つ（`bare_raise`、`defined_paths`、`def_target_scope`、`edge_misc`、
`operator_redefinition`、`operator_visibility`）の `.rc.out` にある。これらは edge-diff-fix で
891d1d9d の出力を期待値にしていたが、rc2 の出力（`.rc.out`）を期待値にし直した。`defined_paths`
は 891d1d9d の形の呼び出し（`__defined_const_path?(nil, [...])`）を直接書いていて rc2 ではそこで
止まるので、rc2 の `mrbc` が出す形（`defined?` の構文）で書き直した。`def_target_scope` は 1 行目で
rc2 が NameError で止まるので、各行を例外のクラスだけ出す `try` で包んだ。

assert ごとの表（891d1d9d の `test/t`）では、rc2 と 891d1d9d が両方通して SabiRuby が落とすものは
4 つだけだった: `float.rb` の NaN（前から意図した差）、`hash.rb` の「entries deleted by an eql?
callback」（M1 で通った）、`syntax.rb` のパターンのキー（前から意図した差）、`string.rb` の
「ASCII only」（SabiRuby の driver ではファイルが途中で止まり、数えられていない）。891d1d9d だけが
通すもの（rc2 では落ちる）は、戻した後の SabiRuby も rc2 と同じく落とす。rc2 のバイトコードで
走らせた列と rc2 の列が違うのは、`codegen.rb` などコンパイラが決めるものだけで、SabiRuby の
コンパイラは rc2 のものなので、そこは rc2 と同じ答えになる。

### 戻したもの

edge-diff-fix のコードのコミット `bd0ff8b`、`fa9419e`、`c28978c`、`301aad1`、`d68a53d`、
`5641a74`、`be14eef`、`8c2432f`、`b26010a` を `git revert --no-commit` で全部戻し、その上で
rc2 の答えのものを足した（`bf61b20`）:

- `Integer#%`: 受け手が 0 なら除数が BigInt でない限り 0（rc2 `int_mod` の `if (a == 0) return x;`）。
  Float の除数 0 は ZeroDivisionError（`flodivmod`、rc2 と 891d1d9d で同じ）。
- `OP_STRCAT`: rc2 の `mrb_obj_as_string`。`to_s` を持たない値（BasicObject）は TypeError
  「can't convert X into String」（`convert_type`）。String でない答えは `mrb_any_to_s`
  （`mrb_type_convert` の String の特別扱い）。
- `Array#[]`/`[]=` の Float の添字: NaN と範囲外は `i64::MIN`。C の `(mrb_int)f` は未定義動作だが、
  本家が走る x86-64 の `cvttsd2si` はこの値を返す（rc2 の出力 `index -9223372036854775808` で確かめた）。
- `Vm::equal`（`mrb_equal`）: 同一、Integer と Float の混在の後、受け手の `==` が組み込みのものでなければ
  送る。組み込みのままなら送った結果と同じなので値で答える（`Vm::eq_builtin_p`、Integer・Float・
  Symbol・nil・true・false の 6 つ。メソッド表の世代番号が変わったときだけ引き直す）。
- `OP_EQ`: 受け手が Symbol で同一でなければ送らずに false（rc2 の `OP_EQ`）。前の SabiRuby は送っていた。

rc2 の mrbtest は 2344 / 2267 / 1979 に戻り、`gem_enum` は 38 に戻った（baseline も戻した）。

### 戻した後も残る、前からの rc2 との差（見つけたが直していない）

`def_target_scope` を rc2 の出力と比べて見つかった。どれも edge-diff-fix の前（`0a433a2`）から
SabiRuby の答えで、今回の作業の範囲の外なので記録だけ:

- `m.class_eval { K2 }` の定数は、SabiRuby は与えたクラス（`m`）から引く。rc2 はブロックの Proc の
  target class（書かれた字句スコープ）から引く（`mrb_vm_const_get`）。891d1d9d も rc2 と同じ答え。
- `private` の下の `attr_*` を SabiRuby は private にする。他のクラスに向けた `other.attr_accessor` を
  `private` のスコープから呼ぶと private にする。rc2 はどちらも public。
- `method_defined?` が 2 つ目の引数を受ける（rc2 は ArgumentError）、private のメソッドを false と答える
  （rc2 は true）。
- `module_function` の下の `attr_reader` を private にする（rc2 はしない）。
- 特異クラスの名前の NameError の文言（rc2 `#<Class:DefInSdef>`、SabiRuby `'#<Class:0x…>'`）と、
  `Array#[]=` の負の添字の IndexError の文言（rc2 `index -5 out of array`、SabiRuby は CRuby の文言）。

## M1. Hash の探索順を本家に合わせる（rc2 の src/hash.c を写す）

### rc2 でも同じ挙動か

計画では master の `src/hash.c` を読むことになっていたが、基準が rc2 に変わったので rc2 と master の
差を先に見た（`git diff 4.1.0-rc2 158760853 -- src/hash.c`、124 行の追加）。差は 3 種類だった:
`entry_check_vacated`（一致したエントリが `eql?` の中で空けられたら例外、`a54353ecf` ほか）、
Symbol の hash code（`28d59b911`）、String のキーを複製する場所（`e368de2ff`）。探索の作り（AR と
HT、探索列、`H_CHECK_MODIFIED`）は同じ。断片で確かめた（同じ 3 つの Hash で `eql?` が呼ばれる相手）:

```ruby
k = Class.new { def eql?(o) $calls << o; false end; def hash; $calls << :hash; 42 end }
h = {"a" => 1, "b" => 2, 3 => 3}; h[k.new]          # rc2・891d1d9d・master とも ["a", "b", 3]
t = {}; 20.times { |i| t[i] = i }; t[k.new]          # 3 つとも [:hash, 9, 4, 17]
t = {}; 20.times { |i| t["s#{i}"] = i }; t[k.new]    # 3 つとも [:hash, "s15", "s10", "s16", "s13"]
t = {}; 20.times { |i| t[:"s#{i}"] = i }; t[k.new]   # rc2 [:hash, :s7]、master [:hash, :s13, :s14, :s2, :s18]
```

AR（16 個まで）では `hash` を聞かずに全エントリと比べ、HT では探索列のエントリと比べる。最後の
行の違いは Symbol の hash code（rc2 は `U32(mrb_fixnum(sym))` で、64 ビットの word boxing では
シンボルの 1 ビットしか残らず、ほぼ全部が同じ探索列に並ぶ）。

master の `hash.rb` を rc2 の `mruby` で走らせると、「entries deleted by an eql? callback」は通り
（rc2 からある 1 つ）、「matched entry vacated」の 3 つは落ち、最後の「carry a pair into a set」で
rc2 は落ちた（解放後の読み出し）。

### 作り

`src/hashtable.rs`（新しいモジュール）。`HashData` は `object.rs` から移し、`object::HashData` は
再輸出にした。

- 本家の EA（entry array）は穴のある配列だが、こちらは `entries` を穴なしの挿入順のまま持つ
  （VM の他の部分は今までどおりスライスで読む）。穴の意味は数で持つ: `slots`（各エントリの本家の
  EA での位置）、`n_used`（本家の `ea_n_used`）、`ea_capa`。これで「いつ詰め直すか、いつ索引を
  作り直すか、いつ AR に戻るか」が本家と同じところで起きる。`ea_next_capa_for`、`ib_bit_for`、
  `ar_set` の容量の分岐、`ht_set` の `goto compress` まで写した。
- HT の index buckets は、本家はビット詰めの配列だが、こちらは `Vec<u32>`（空・削除・`entries` の
  位置）。探索列は `ib_it_next` と同じ三角数の歩み。エントリを消すと後ろの位置が 1 つずれるので、
  その位置を指す bucket を書き換える（O(容量)。隣の `Vec::remove` も O(n)）。
- hash code は `mrb_obj_hash_code`: String は FNV-1a 32 ビット、nil・false・true は word boxing の
  `mrb_fixnum`（0・2・6）、Integer は下位 32 ビット、Float は -0.0 を 0.0 にして 8 バイトの FNV、
  それ以外は `hash` を送って `U32(tt) ^ U32(答え)`（`tt` は本家の `mrb_vtype` の番号を ObjKind から
  当てた）。最後に `h ^= h>>16; h *= 0x45d9f3b; h ^= h>>16`。Symbol だけはシンボルの番号を使う
  （SabiRuby の番号は本家と違うので、どう写しても同じ探索列にはならない。rc2 の「ほぼ全部衝突」を
  写すと大きな Symbol キーの Hash の探索が O(n) になる）。
- `H_CHECK_MODIFIED`: `HCheck`（bucket の幅、`ea_capa`、件数、`replace`/`clear` の世代、AR では
  EA の先頭のキー）。最後のものは、本家が word boxing の 64 ビットで AR の EA の先頭を
  `h->hsh.ht->ea` として読むことの写し（master のテストの注釈「only a hole ahead of that slot exposes
  the AR path」が言っていること）。
- `obj_eql`: 探す側のキーが String・Symbol・Integer・Float ならその場で比べ、それ以外は `mrb_eql`
  （同一なら真、Object の `eql?` のままなら偽、それ以外は `eql?` を送る）をチェックの中で。
- 一致したエントリが `eql?` の中で空けられた場合（rc2 は検査しない）: rc2 はスロットに残った値を
  読む。`Found::Vacated(値)` にして、読み出しはその値、書き込みはどこにも書かない、削除は値を
  返して何も消さない。rc2 の削除はここで件数を二重に減らすが、それは写していない（表の扱いに）。
- 走査（`assoc`、`rassoc`、`==`、`eql?`、`inspect`、`__except`、`merge`/`__merge`、`rehash`、
  `shift`）も rc2 の `H_EACH` とチェックの形にした。`assoc`/`rassoc` は前は `==` で比べていたが、
  rc2 は `obj_eql`（格納された側が答える）。`merge` にブロックがあるときは rc2 の mrblib の形
  （`dup` を送り、各キーで `self[k]` と `other[k]` を読む）。
- `OP_HASH` とキーワード引数の Hash は `mrb_hash_new_capa(n)`（17 組以上のリテラルは最初から HT）。
- Set（mruby-set の khash）: 4 個までは表を持たず全要素と比べ、格納された要素が `eql?` に答える。
  5 個目で 8 bucket の表を作り、`hash` から `k + ++step` で歩く。7/8 で 2 倍に作り直す。作り直しが
  `eql?` の中で起きたら RuntimeError「hash table modified during callback」（`KHASH_CHECK_MODIFIED`）。
  要素の順は前と同じ挿入順のまま（本家は bucket 順。前からの差）。

### 確かめたこと

- `tests/custom/hash_lookup_order.rb`（新規）: 上の断片に、リテラル 18 組、17 個目で索引を作る
  ときの `hash` と `eql?` の順、`assoc` で格納側が答えること、HT の `shift` が `hash` を聞くこと、
  `rehash`、`eql?` が消したときの例外、Set の小さい表を足した。rc2 の出力と一致（Symbol は入れていない）。
- `tests/custom/hash_eql_raises.rb` の「hash raises 3」は、前は「SabiRuby は `hash` を聞く」として
  rc2 と違う期待値だったが、AR では `hash` を聞かないので rc2 の出力（nil / false）に揃った。
- rc2 の mrbtest: `hash` 45/45（前 44）、`gem_set` 42/42（前 41）。既定 2346 / bytes 2269 /
  no-regexp 1981（各 +2）。891d1d9d の `hash.rb`（edge_compare のもの）は 46/49 で、落ちる 3 つは
  rc2 の `mruby` も落とす「vacated」の 3 つ。

### 性能

A は戻した状態（`bf61b20`）、B は M1。最初の形では `ds_hash` が +18% だった。Integer のキー
1000 個の Hash で、探索のたびに `self.heap.get(o)` を何度も引き直し、1 回ごとに関数を渡り歩いて
いた。String・Symbol・Integer・Float のキーは Ruby が関わらないので、探索列を 1 回の借用の中で
歩く道（`HashData::find_native`、`probe_native`、作り直しの `h_rebuild_native`）を足した。同じ
エントリを同じ順に見るだけで、違うのは手間だけ。`ds_hash` は +3.1%、`app_json_hash` は −2.1%。

27 本では `gc_churn` +4.4%、`vmo_objects` +4.0% が線を超えた。`{}` を 1000 万回作るだけのループで
545 ms → 620 ms。`--stats` で GC の時間は同じ（112 ms と 118 ms）で、命令ループの側だった。
`OP_HASH` の腕で `HashData::with_capa(b)` を呼ぶようにしたのが原因で、空の Hash は前と同じ
`hash_new()` に、1 組以上だけ `with_capa` にしたら 553 ms に戻った。外に出す（`#[inline(never)]`）
のは逆に悪かった（664 ms、本体を全部外に出すと 767 ms）。A/A（戻した状態の複製同士、7 巡）と
並べて:

| benchmark | A/A | M1 |
|---|---:|---:|
| gc_churn | +0.3 | −0.5 |
| vmo_objects | −1.9 | +2.8 |
| ds_hash | −0.3 | −0.3 |

`vmo_objects` の残りは String のキー 5 万個の Hash で、索引を作り直すたびに全キーの FNV を計算し
直すため（本家も同じことをする。前の SabiRuby は code をエントリごとに覚えていた）。線の内側。

## M2. 性能の戻り

見立て 1〜3 は edge-diff-fix の HEAD（`d7b1dd4`）の戻りについてのもの。

1. **`ObjKind`／`HeapObject` が大きくなったか**: 外れ。`main`（`8a3540a`）と edge-diff-fix の HEAD で
   `size_of` は同じ（`ObjKind` 232、`HeapObject` 264、`ProcData` 48、`EnvData` 120、`HashData` 128、
   `ClassData` 232）。`ObjKind` の大きさは `ClassData` で決まっていて、`ProcData` の bool 2 つは
   詰め物に入っていた。M1 の後は `HashData` 104 で、`ObjKind` は 232 のまま。
2. **`arith_value`／`op_compare` の道**（`app_robot`）と 3. **cref の歩き方**（`bm_ao_render`）: 両方とも
   D1 と D4 のコードで、rc2 に戻したときに消えた。

A は edge-diff-fix の前（`0a433a2`）、B は今（M1 の後、`sabiruby` の中身は `bab3b3b`）。`tools/bench_ab.sh`、
27 本、P コア 2、1 巡 5 回、best 同士の変化（%）。A/A は A のコピー同士。

| benchmark | A/A #1 | A/A #2 | A/B #1 | A/B #2 | A/B #3 |
|---|---:|---:|---:|---:|---:|
| app_json_hash | +0.7 | +0.5 | -6.3 | -8.3 | -3.7 |
| app_robot | -1.1 | +3.4 | +2.6 | +3.2 | +3.5 |
| app_tak | +0.9 | +1.2 | +0.4 | -0.3 | +3.4 |
| bm_ao_render | -0.0 | +1.6 | +0.1 | +0.8 | +2.1 |
| bm_fib | -0.3 | -0.1 | +0.8 | +1.5 | +1.4 |
| bm_mandel_term | +0.0 | -3.1 | -0.1 | -1.7 | +1.7 |
| bm_so_lists | +0.4 | +0.6 | +0.7 | +0.4 | -2.5 |
| bm_so_mandelbrot | +0.3 | -1.3 | +0.0 | +0.5 | +1.3 |
| call_args | -0.4 | -4.0 | -0.1 | -1.3 | -5.6 |
| call_block_yield | +0.7 | +0.7 | -2.0 | -4.1 | +4.5 |
| call_fiber | -5.8 | -8.5 | +1.9 | -3.1 | -2.8 |
| call_kwargs | +0.3 | +3.9 | -12.3 | -5.2 | -12.9 |
| ds_array | -1.1 | +2.9 | +2.2 | +4.5 | +1.0 |
| ds_hash | +0.2 | +3.5 | -0.6 | +16.8 | +0.2 |
| ds_string | -0.8 | -2.1 | +0.0 | +3.6 | -0.2 |
| gc_churn | +0.1 | +0.4 | -0.3 | -6.9 | +0.3 |
| loop_if_branch | -0.1 | -7.3 | -1.2 | -1.1 | +0.5 |
| loop_times | +0.1 | +1.4 | -1.2 | +3.5 | -1.2 |
| loop_while_add | +0.0 | -3.6 | -1.0 | +2.9 | +0.5 |
| mem_retained | +1.0 | +0.5 | +5.5 | -0.8 | -2.1 |
| mem_short_lived | -0.8 | +1.0 | +6.8 | +11.7 | +0.8 |
| vm_optimization_bench | -0.6 | +4.7 | -0.2 | -3.5 | +1.0 |
| vmo_arith | +0.3 | +2.0 | -1.7 | -1.0 | -0.7 |
| vmo_calls | -1.3 | +0.9 | -0.8 | +0.5 | +8.4 |
| vmo_dispatch | +0.3 | +2.8 | -0.2 | +0.0 | -2.3 |
| vmo_index | -1.0 | -1.6 | -0.7 | +1.6 | +1.2 |
| vmo_objects | -3.7 | +6.1 | +5.2 | +4.1 | +0.3 |
| 合計（ms の和） | -0.4 | +1.5 | -0.2 | -0.6 | +0.8 |

A/A の幅が今日は大きい（#2 で `call_fiber` −8.5%、`loop_if_branch` −7.3%、`vmo_objects` +6.1%）。
A/B の #2 と #3 の間に、同じバイナリで `mem_short_lived` が 890 ms から 1040 ms に動いた時間帯が
あった（上の「計測」）。3 回とも同じ向きで A/A の幅を超えたものは無い。いちばん一貫しているのは
`app_robot` の +2.6・+3.2・+3.5 で、A/A（−1.1・+3.4）の幅の中。速くなったのは `call_kwargs`
（−12.3・−5.2・−12.9: キーワードの Hash を `mrb_hash_new_capa` の大きさで作り、AR の探索で `hash`
を聞かない）と `app_json_hash`（−6.3・−8.3・−3.7）。edge-diff-fix で線を超えていた
`bm_ao_render`（+0.1・+0.8・+2.1）、`app_robot`、`gc_churn`（−0.3・−6.9・+0.3）は、D1 と D4 を
戻したので前の水準に戻った。`MRB_CONST_CACHE` を入れる案は、cref の歩き方を戻したので要らなくなった。

## M3・M4

止めた（冒頭）。着手前だったので戻すものは無い。`kishima/mruby:15876085` のイメージと
`../../ref/mruby-158760853` は残してある（比較と、この記録の確かめに使った）。

## M5. 再計測と、C fixture ありの列

`tools/edge_compare.sh`（テストとコンパイラは 891d1d9d のまま）:

| | edge-diff-fix の前（`0a433a2`） | edge-diff-fix の後（`d7b1dd4`） | 今（rc2 に戻し、M1） |
|---|---:|---:|---:|
| 全部通ったファイル | 29 / 48 | 35 / 48 | 29 / 48 |
| pass | 856 | 897 | 857 |
| fail | 35 | 8 | 34 |
| error | 34 | 20 | 34 |
| 参考列（本家 `assert.rb`） | ok 882、ko 45、crash 30 | ok 936、ko 7、crash 14 | ok 883、ko 44、crash 30 |

前（`0a433a2`）との差の 1 は `hash.rb` の「entries deleted by an eql? callback」（M1）。

著者の依頼で、SabiRuby の列をもう 1 つ足した: mruby/edge の `helpers.rb` と数え方はそのままで、
SabiRuby の `mrbtest` が持つ本家テスト用 C fixture の移植（`sabiruby::mrbtest::install`:
`TestNotImplement`、`__env_svar?`、`Mrbtest.nofree_cstr?`、`TestVFormat` の代わりのもの など）も
定義して走らせたもの。`sabiruby edge-spec --with-fixtures`（fixture を先に定義し、driver の
`t_print`・`_str_match?` を Object に後から置くので driver のものが呼ばれる）、
`tools/edge-compare/run.py --with-fixtures`、出力は `sabiruby-fixtures.json`、ログは
`target/edge-compare/logs-fixtures/`。ページは Summary・Files・Asserts に 3 列目、Conditions に
「SabiRuby の列は mruby/edge と同じ条件、C fixture ありの列はそうでない」と、rc2 の挙動を守るので
master で変わった挙動のテストは落ちる、の 2 つの事実。Asserts の対照は、いちばん先まで走る
fixture ありの列に他の 2 列を合わせた。

| | SabiRuby | SabiRuby（C fixture あり） | mruby/edge |
|---|---:|---:|---:|
| 全部通ったファイル | 29 | 29 | 9 |
| pass | 857 | 883 | 119 |
| fail | 34 | 44 | 36 |
| error | 34 | 30 | 234 |

C fixture ありの列は `syntax.rb` が最後まで走る（TestNotImplement で止まらない）ので fail が増え、
数は参考列（本家の `assert.rb`、同じ fixture）の ok 883・ko 44・crash 30 と一致した。`env.rb` は
fixture ありでも落ちる: SabiRuby が移植しているのは `__env_svar?` と `__env_svar_slot` だけで、
このテストが呼ぶ `__env_svar_read` などは無い。

`tests/edge-spec/notes.tsv` は、落ちる assert ごとに「master で stable の分岐点の後に変わった挙動、
SabiRuby は rc2 のもの」とコミットを書いた。`syntax.rb` の `defined?` と 15 個の引数、
`deconstruct` の可視性の多くは、891d1d9d のコンパイラが出すバイトコードに rc2 の VM が対応して
いないもの（rc2 の `mrbc` でコンパイルした同じテストは rc2 と同じに通る）。

本の側（`book_mruby3/docs/notes/`）への 1 行は、計画では「SabiRuby が master 基準になった」と書く
ことになっていたが、master 基準にはしなかったので書いていない。

## 気づいた点

- **VM（sabiruby）/ rc2 との差**: 「戻した後も残る、前からの rc2 との差」の 5 つ（`class_eval` の
  ブロックの定数、`attr_*` の可視性、`method_defined?` の引数と private、`module_function` の
  `attr_reader`、2 つの文言）。edge-diff-fix の前から SabiRuby の答えで、rc2 の mrbtest には出ない。
  直すなら別の計画（`class_eval` の定数は、本家の `MRB_PROC_TARGET_CLASS` から引く順に変えることに
  なり、D4 で一度やった cref の作りの rc2 版）。
- **VM（sabiruby）/ Set**: 要素の順は挿入順のまま（本家は khash の bucket 順。`[30, 5, 17, 2, 99, 1, 64,
  8, 3]` を入れると本家は `[30, 5, 8, 99, 1, 64, 3, 17, 2]`）。`Set#clear` で表が小さい形に戻る
  （本家の `kh_clear` は bucket を残す）。
- **VM（sabiruby）/ Hash**: rc2 の `mrb_hash_compact` は EA のエントリを消すが bucket を残し、その後の
  探索で消えたエントリ（undef のキー）に `eql?` を聞きうる。こちらは bucket も消している。
- **計測**: この PC は午後、同じバイナリ同士の A/A が ±7% まで振れる時間帯があった（`vmo_objects`
  +7.0%、`mem_retained` +7.5%）。`vmstat` の idle は 99〜100% で、Linux の中からは原因が見えない。
- **計画書の前提**: M2 の見立ては edge-diff-fix のコードについてのもので、rc2 に戻したことで 2 と 3 の
  対象は無くなった。`size_of` は main とも同じだった。
- **本の素材**: 本家の Hash の探索順（AR は全部、HT は探索列）は Ruby の `eql?` から観測でき、
  `Hash` を移植する VM はハッシュ値の比較で `eql?` を減らす CRuby 流の作りでは本家と同じ答えに
  ならない。stable と master の分岐（4.1.0-rc2 は master の祖先ではない）も、移植の基準の選び方の
  材料になる。
