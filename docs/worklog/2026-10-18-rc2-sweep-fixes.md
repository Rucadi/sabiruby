# 2026-10-18 rc2-sweep の差を直す（A・B と C の記録）

計画: `../plans/rc2-sweep-fixes-plan.md`。差の表: `../verification/rc2-sweep/README.md`。前の調査の記録:
`2026-10-18-rc2-sweep.md`。

## Y0. 洗い出しの道具を常用の形に

基準: main `e70ed76`。VM・CLI・ほかの crate のコードは変えていない。変えたのは `tools/rc2_sweep/` と docs だけ。

### 比べる相手を引数に

`sweep.py` の `IMG` と `programs.py` の `IMAGES` は rc2 の像の名前を直に書いていた。`sweep.py` は `lists` と
`run` に `--image`（既定 `kishima/mruby:4.1.0-rc2-utf8`）、`programs.py run` は `--image`（同じ既定）と
`--bytes-image`（既定 `kishima/mruby:4.1.0-rc2`、tests/custom の `# utf8-only:` の無いもの用、`tools/custom.sh` の規則）
を受けるようにした。既定の名前は `DEFAULT_IMAGE`／`DEFAULT_IMAGES` の 1 か所だけに残した。使う像は走らせた
ときの最初の行に出す（あとでログから、どの像と比べたかが分かるように）。`sweep.py` の `lists` と `run` には同じ像を
渡す必要がある（呼ぶ表は `lists` の一覧から作るので）。help にそう書いた。

`sweep.py cases` は Docker を使わないので `--image` を受けない。

### 前回との差: `compare.py diff`

`compare.py` は縮めた表を `docs/verification/rc2-sweep/` に直接書き直す。だから「前回」はコミットしてある表で、
`git show HEAD:docs/verification/rc2-sweep/<file>` で読めば、別にコピーを取っておかなくてよい。そう決めて、
`compare.py diff [--base REV|DIR]` を足した。既定は HEAD、別のコミットやディレクトリも渡せる。

- X1: `x1-counts.md` の「比べた数／答えの無い数」、`x1-causes.tsv` の原因ごとの calls・v・e・m・k の増減、
  新しく出た原因、消えた原因。ついでに `x1-methods.tsv` の Class#method の行の増減・変化も出す（原因の数が
  同じでも、中身が別のメソッドに入れ替わったときに見えるように。数行で済むので足した）。
- X2: `x2-programs.tsv`（program が鍵）と `x2-asserts.tsv`（program・seq・assertion が鍵）の行の追加・削除・変化。
  `x2-causes.tsv` は手で書いた表なので比べない。
- 何も変わらなければ `no change` で終わり、終了コード 0。変われば `changed`、1。

確かめ: HEAD の表に対して何も走らせずに `diff` を打つと `no change`。スクラッチパッドに表をコピーして、
原因の数を 3 増やし、原因の名前を 1 つ変え、assert の行を 1 つ消したものを `--base` に渡すと、
「changed 1・new 1・gone 1・x2-asserts の added 1」と出て終了コード 1（出力は下）。

```
X1 causes: 92 -> 92; 1 new, 1 gone, 1 changed
  new	argc-splat	C	+12042	+0	+12042	+0	+0
  changed	internal-helper	D	15181->15178 (-3)	179->176 (-3)	8766	6236	0
  gone	old-cause	C	-12042	-0	-12042	-0	-0
X2 x2-asserts.tsv: 249 -> 250 rows; 1 added, 0 removed, 0 changed
  added: mrbtest/gem_array.rb 7 Array#uniq, Array#- and Array#include? with a NaN (result)
changed
```

### 道具の数を `docs/numbers.md` に

`--timeout`（X1・X2）、`--jobs`、`--mem` の既定、`SW_YIELD_MAX`、3 個の引数のラテン方格を 1 行ずつ載せた。
出どころは前の worklog から引いた。書かれていなかったものは「unknown」とした:

- X1 の最初の全体の走行の 60 秒、X2 の 60 秒: どこから来たかは書かれていない。X1 で使う 5 秒は「終わった最も遅い
  呼び出し 0.112 秒の 40 倍以上」と導いてあるので、それを書いた。X2 の 60 秒は、走らせた後で最も遅いものが
  1.33 秒だったことだけを書いた。
- `--mem` の「`MemAvailable` の半分」: 式はあるが、なぜ半分かは書かれていない。
- ラテン方格: どの 2 つの位置にもどの 2 つの値の組が出る、という性質は導出。3 個を全部（22 倍）回さなかった理由は
  測っていない。

`SW_YIELD_MAX` とラテン方格は引数（コマンドの option）にはしなかった。どちらも呼ぶ表（`prelude.rb` の値、
`cases.py` の `NVALS`・`MAXARGS`）から決まる数で、表を変えるときに一緒に変えるものなので、変える場所は
そのファイルとして載せた。`--timeout` は今までどおり既定値を置かない（毎回渡す）。

### 確かめ: 直しの無い main で回し直す

`e70ed76` と前の調査の基準 `f009316` の間に VM・CLI のコードの変更は無い（`git diff --stat` は docs と
`tools/rc2_sweep` だけ）。`cargo build --release -p sabiruby-cli` で建て直した（md5 は前の `57759bd1…` から
`d2cfc20d…` に変わった。コードは同じで、建て直しで変わる部分だけと見ている。確かめてはいない）。前の作業の
ディレクトリは `target/rc2-sweep-prev/` に移して残し、新しい `target/rc2-sweep/` で `lists` から回した。
`lists` の 4 つの一覧、`cases` の `chunks.json`・`one_side.tsv`・`skipped.tsv`、`(1..)` の塊のソースは、前と
バイト単位で同じだった（`cmp`）。

`(1..)`（受け手 `Range:endless`）は `--only '^(?!Range:endless$)'` で外し、その受け手の raw は前の走行のものを
`target/rc2-sweep-prev/raw/{ref,sabi}/Range_endless.tsv` からコピーした。外すだけだと、前回答えのあった
12,043 呼び出しが「答えなし」に移り、`diff` がそれを変化として出すため。前回の答えをそのまま使うので、
この受け手は確かめていない。

**1 回目は止めた。** `--timeout 5` で回すと、30 分たっても rc2 側の String の 5 つと Array の 4 つ
（47,216 と 43,598 呼び出しの塊）が終わらなかった。raw を見ると全部の行が `timeout` で、1 行も答えが無い。
`Chunk.run`（`sweep.py`）は、子を起こしてから最初の行が来るまでも 1 呼び出しの上限で待っていた。rc2 は
`mruby chunk.rb` でソースを読んでから `ready` を書くので、その塊の読み込みは静かなときで約 4 秒
（`docker run … mruby String_abc.rb 47216` で `ready` と `end` が出るまで 4.25 秒、docker の起動込み）、
18 本並べると 5 秒を超える。超えると最初の呼び出しが timeout になり、次の呼び出しから起こし直して
また読み込みで超える、を全部の呼び出しで繰り返す。前の調査で全体を回したときは上限 60 秒だったので
当たらず、5 秒は小さい `(1..)` の塊にだけ使っていた（README の「`run --timeout 5` で全部」は、書いた
とおりには回っていなかった）。`Hash:default` と `Hash:frozen` にもこのとき 4 と 7 の timeout が出ていた。
前の raw には `(1..)` 以外の timeout は無い。

直し: 上限を `ready` の行から数える（それより前は待つだけ）。新しい数は置かない。読み込みで止まったまま
になる場合は走行が終わらないので目で分かる。最も遅い呼び出しの記録（`slowest`）も `ready` から数える
ようにした（前は起こし直した直後の 1 呼び出しを数えていなかった）。

**2 回目と 3 回目**（直した道具で、全体を 2 度）:

| | X1 `run` | X2 `run` | `compare.py diff`（HEAD と） |
|---|---|---|---|
| 2 回目 | 72 s（51 restarts、最も遅い呼び出し 0.475 s） | 72 s（終わったもので最も遅い 1.73 s） | changed: `time-now` が消えた（2 呼び出し）、`book/vm_task_sleep_switching.rb` の `sabi_out_len` 297 → 298 |
| 3 回目 | 75 s | 72 s（最も遅い 1.63 s） | **no change** |

restarts の 51 は SabiRuby の String の塊の crash（`"abc".bytesplice(0, 0)` などの panic、前の raw と同じ
12・12・12・12・3 本）。X2 で上限に当たったのは前と同じ `test_NOP_empty_loop.rb` の 4 つだけ。
`lists` 1 s、`cases` 8 s、`compare.py` 8 s。走らせたときの `vmstat` の idle は 55〜95%（隣で別の作業あり）。

2 回目に出た 2 つはどちらも走らせ方の揺れ:

- `time-now`: `Time.at(0).utc.initialize`（引数なしで今の時刻）の 2 呼び出し。前の調査では rc2 が 19:16:11、
  SabiRuby が 19:16:10 で差になっていた（両側の塊は別の時刻に走る）。2 回目は両側が同じ秒に入って「同じ」に
  なり、原因ごと消えた。3 回目はまた秒をまたいで、前と同じ 2 呼び出しになった。
- `vm_task_sleep_switching.rb`: 壁時計のミリ秒を印字する本のプログラム（`[:other, 2]` …）。rc2 の像には
  Task が無いので rc2 側は毎回同じ例外で、SabiRuby 側の出力の長さだけが 1 バイト動く。`differs` の分類は
  変わらない。

呼び出しの単位でも 2 回目と 3 回目の raw を比べた（アドレス `0x…` を消してから）。表の数には出ないが、
答えが毎回変わる呼び出しは次のとおり（どれも両方の回で「差」に分類されていて、原因も同じ）:

| 側 | 受け手 | メソッド | 呼び出し | 何が変わるか |
|---|---|---|---:|---|
| rc2 | `Array:empty` | `fill`、`__fill_parse_arg` | 45、38 | `[].fill(0, 1..3)` が未初期化のメモリを読む値（`fill-empty-range`、C にする予定のもの） |
| rc2 | `Integer:0`・`7`・`-3`・`big` | `to_s`、`inspect` | 各 1、1 | 数でない基数で "invalid radix <アドレスらしき数>"（k: Integer#to_s の基数） |
| rc2 | `Set:int` | `add_all` | 64 | Proc を含む Set の並び（Proc のハッシュがアドレス） |
| rc2 | `Proc`、`Proc:lambda` | `hash` | 各 2 | ハッシュの値（k） |
| rc2 | `Object` | `object_id`、`__id__`、`hash` | 各 2 | `object_id` は `<id>` に正規化して比べるので分類は変わらない。`hash` は k |
| 両側 | `Time`、`Time:usec`、`self:Time` | `initialize`、`now` | 各 2 | 今の時刻 |

`x1-causes.tsv`・`x1-methods.tsv` の例の列（rc2 の答え）は、このうち `fill`・`hash`・時刻を例に選んだ行が
毎回書き変わる。`diff` は例の列を比べないことにした（数だけを比べる）。表は git で毎回 4〜6 行動くので、
このラウンドの確かめでは、生成した X1 の表をコミットせずに HEAD に戻した（数は同じで、例の列だけが違った）。

**`x2-asserts.tsv` の並びも毎回変わっていた。** 1 つのファイルの中で同じ番号（seq）の assert が 2 つある
とき（`gem_cmath.rb` の `CMath.exp` と入れ子の `assert_complex`）、`programs.py compare` は番号だけで並べ、
同じ番号どうしは Python の set の順（文字列のハッシュは走らせるたびに変わる）になる。並べる鍵に名前を足して、
`PYTHONHASHSEED` を 1 と 2 にして同じ md5 になることを確かめた。コミットした `x2-asserts.tsv` はこの並びで
書き直した（1 行の位置が動いただけ、`diff` は no change）。

### 所要時間

X1 と X2 を合わせて約 3 分（上の表）。計画は「全体で数十分」と見積もっていたが、それは前の調査の 1 回目
（上限 60 秒で `(1..)` も含めた）からの見積もりで、`(1..)` を外せば 1 回 75 秒で済む。X2 の 72 秒のうち 60 秒は
終わらないプログラムが上限に当たるのを待つ時間。`docs/verification/rc2-sweep/README.md` の Tools と
`docs/README.md` に、いつ回すか（段階の終わり、本家の新しいバージョンへの追従）と一緒に書いた。

## 気づいた点

- **X1 の時間の上限が読み込みを数えていた**（`tools/rc2_sweep/sweep.py` `Chunk.run`、直した）。前の調査の
  README と worklog の「`run --timeout 5`」は、そのままでは String・Array を全部 timeout にする。前の結果は
  上限 60 秒の走行のものなので、結果そのものは正しい。属する先: 道具（直した）。
- **時刻と壁時計の揺れは道具では消していない**: `time-now` の 2 呼び出しと `vm_task_sleep_switching.rb`。
  消すなら `compare.py` の正規化で `Time.now` 相当の答えを `<now>` にする、`programs.py` で長さの列を比べない、
  などがある。どちらも比べ方を変えるので、ここでは揺れとして README に書くだけにした。属する先: 道具／著者。
- **rc2 の `Integer#to_s` に数でない基数を渡すと、基数の文言にアドレスらしき数が出る**（"invalid radix
  108509866934832"、毎回変わる）。k（記録済み）の規則に入っているが、rc2 は値を確かめずにポインタの値を
  基数として読んでいるように見える。追っていない。属する先: 本家の不具合の候補。
- **`(1..)` の 4,345 呼び出しは前の調査から一度も答えが無い**。外すのを `--only` の否定先読みで書くのは
  分かりにくいので、常用するなら `--skip` を足してもよい（今回は計画にないので足していない）。属する先: 道具。
- **`SW_YIELD_MAX` とラテン方格は option にしていない**（`prelude.rb`、`cases.py`）。呼ぶ表から決まる数として
  `numbers.md` に「変えるのはそのファイル」と書いた。option にするかは著者の判断。属する先: 道具。

## Y1. 危ないもの 3 つ（`crash`、`setmcnst-frozen`、`tdef-no-target`）

基準: main `9e6e44b`。

### `crash`: `"abc".bytesplice(0, 1)` の panic

`src/builtins/string.rs` の `bytesplice`（Y0 の時点で 541 行）は、最初の引数が Range でなければ前の 2 つを
添字と長さに取り、残り（`rest`）の先頭を置き換える文字列として読む。引数が 2 つで最初が数のとき `rest` は
空で、`rest[0]` が範囲外になって panic していた。

同じ関数のほかの添字の使い方も見た。`rest[1..]` は `rest.len() >= 2` のときだけ読む。`index_args`
（同ファイル 249 行）は 1 つの数なら `i < len`、2 つなら `i <= len` と `n <= len - i` に切り詰め、Range なら
`range_beg_len(…, trunc = true)` が `beg <= len`・`end <= len` にするので、`rep[ri..ri + rn]` と
`nb.splice(i..i + n, …)` は範囲の中に収まる。引数が 5 つで最初が Range のときは `index_args` が 3 つを
受けて ArgumentError になる（panic ではない）。確かめに、16 個の値（数、nil、文字列、いくつかの Range、
Float、Symbol）の 2〜5 個の並びで 163,512 回呼んで、panic が一度も無いことを見た（4 個と 5 個は 7 つに 1 つ
だけ）。`range_beg_len` の `end += 1` は `end` が `i64::MAX` だと溢れるが、release は溢れを検査しない
（`Cargo.toml` の `[profile.release]` に `overflow-checks` は無い）ので値が巻き戻るだけで、
切り詰めの後の長さは 0 になる。debug でも上の並びの `(2**62)..(2**62)` などでは当たらなかった。
これは `bytesplice` ではなく `range_beg_len` の話なので、下の「気づいた点」に書いた。

止め方は rc2 の読み方に寄せた。rc2 の `mrb_str_bytesplice`（src/string.c:4422）は引数が 2 つのとき
`mrb_get_args(mrb, "oS", …)` で 2 つ目を先に String にする（TypeError "Integer cannot be converted to
String"）。2 つ目が String なら `mrb_range_beg_len` が最初の引数を Range でないと答えて、`default` に落ちて
ArgumentError "wrong number of arumgnts"（rc2 の綴りのまま）。この 2 つの道は短いので、panic していた
枝（引数 2 つ、最初が Range でない）では rc2 と同じ答えを返すことにした（冷たい関数
`bytesplice_two_args`）。ほかの形（数を先に見る、境界の検査の順など）は Y2 に残した。洗い出しでは、
`bytesplice` の差が 4,434 → 2,175 呼び出しに減った（2 つの引数の呼び出しが rc2 と同じになった分）。

### `setmcnst-frozen`: 凍結したクラスへの `A::Y = 2`

rc2 の `OP_SETMCNST`（src/vm.c:3389）は `mrb_const_set(mrb, regs[a+1], sym, regs[a])` を呼ぶだけで、
`mrb_const_set`（src/variable.c:1458）が `mod_const_check` → クラスなら名前付け → `mrb_obj_iv_set`
（`mrb_check_frozen`、src/variable.c:685）の順に進む。凍結していれば `frozen_error`
（src/error.c:652）の `"can't modify frozen %T"`。SabiRuby の `Op::Setconst` は既に
`self.frozen_error(...)`（同じ文言、`describe_for_error` でクラスの名前だけ）を書いていたので、
`Op::Setmcnst` も同じ形にした（`src/vm.rs` の `Op::Setmcnst` の腕に 1 行。文言は `frozen_error` の中で
組むので腕には入れていない）。

rc2 は凍結の検査の前に `mrb_class_name_class` で値の無名クラスに名前を付けるので、
`c = Class.new; A.freeze; (A::B = c) rescue nil` の後 `c` は "A::B" になる。SabiRuby の `Setconst` も
`Setmcnst` も凍結を先に見る。`Setconst` に合わせてこの順のままにした（表に無い差として下に書いた。
確かめてはいない）。

本の `data/code/exp_GETCONST_frozen.rb` を rc2 と SabiRuby で走らせ、出力が一致した
（`FrozenError` / `"can't modify frozen Class"` が 3 回）。

### `tdef-no-target`: `1.instance_eval { def x; end }`

rc2 の `check_target_class`（src/vm.c:2413）は `CI_TARGET_CLASS(ci)` が NULL なら TypeError
"no class/module to add method"。呼ぶのは `OP_TDEF`（4628）、`OP_TCLASS`（4653）、`OP_ALIAS`（4660）、
`OP_UNDEF`（4669）の 4 つ。`OP_TCLASS` は `def` の子 irep が 256 個目より後のとき `TDEF` の代わりに出る
（mruby-compiler codegen.c:5131、`TCLASS + METHOD + DEF`）。NULL になるのは `mrb_singleton_class_ptr`
（src/class.c:2619）が Integer・Float・Symbol（と CPTR）に NULL を返すときで、それを渡すのは
`instance_eval`（ブロック: `mrb_obj_instance_eval`、文字列: mruby-eval `f_instance_eval`）と
`instance_exec`（mruby-object-ext）。nil・true・false は NilClass などを返すので当たらない。

SabiRuby では、フレームのクラス（`CallInfo::target_class`）は `ObjId` で、「無い」を表せなかった。
`exec_block_with_self` などは Integer などのとき `None` を渡し、`None` は「ブロックの環境のクラス」と
読まれていたので、`def` はブロックが書かれたクラス（トップレベルなら Object）に入り、`undef to_s` は
Object から `to_s` を取り去っていた（後の `alias z to_s` が "undefined method 'to_s' for class Object"）。

rc2 で、NULL がどこまで伝わり、何がそれを読むかを先に確かめた（スクラッチパッドの 3 つのプログラム、
40 行ほど）。分かったこと:

- ブロックの中で作ったブロック・proc・lambda も NULL を持つ（フレームの環境が `CI_TARGET_CLASS` を
  写す）。後で `pr.call` しても TypeError。`String.class_eval(&pr)` で呼べば String に入る。
- `eval("def …")` はブロックの直下では通り（mruby-eval は呼んだ proc の `MRB_PROC_TARGET_CLASS` を使い、
  それはブロック自身のクラス）、その中のブロックからでは TypeError。`binding.eval` は TypeError。
- 定数の参照・代入と `class` は NULL を Object と読む（src/variable.c の `if (!c) c = mrb->object_class`）。
  ブロックの直下の `W = 1` と `class C; end` は、`OP_SETCONST` と `OP_CLASS` がフレームではなくブロックの
  proc のクラスを読むので、書かれたクラス（K）に入る。その中のブロックや文字列では Object に入る。
- クラス変数は NULL を特異クラスと同じく飛ばして外側を探す（`mrb_vm_cv_get`）。
- `Module.nesting` は文字列でも `[K]`。

**選んだ形**: 「クラスが無い」を表す隠れたモジュールを 1 つ作り（`Vm::no_target`、名前なし、
モジュールなので起動時の metaclass の作成から外れる、GC の根）、rc2 が NULL を置く所にそれを置く
（`exec_block_with_self`、`call_block_with_self_kw`、`instance_eval` の文字列）。フレーム・環境・proc は
今までどおり `ObjId` を写すだけなので、中のブロックにも自然に伝わる。読む側は:

- `TDEF`・`ALIAS`・`UNDEF`・`TCLASS`: 冷たい関数 `no_target_error` で TypeError。
- `proc_target_class`（`MRB_PROC_TARGET_CLASS` 相当）: 隠れたモジュールなら Object を返す。定数の探索
  （`const_class`、`const_search` の外側の proc）、`OP_CLASS` の外側、`Module.constants` はこれを通す。
  `const_class` は、フレームのクラスが隠れたモジュールのとき proc のクラスを読む（debug の
  `debug_assert` がフレームと proc のクラスの一致を見ているため）。
- `OP_SETCONST`: SabiRuby はフレームのクラスに書く（rc2 は proc のクラス。下の気づいた点）。フレームの
  クラスが隠れたモジュールのときだけ、rc2 と同じく proc のクラス（無ければ Object）に書く。
- `cvar_class`: 特異クラスと同じく飛ばす。
- `Module.nesting`: 載せない。`ObjectSpace.each_object`: 数えない。
- `eval`（`eval_proc`）: フレームのクラスが隠れたモジュールなら、呼んだ proc のクラス（生の値、中の
  ブロックなら隠れたモジュールのまま）を使う。

捨てた案: `CallInfo::target_class` を `Option<ObjId>` にし、`ProcData`・`EnvData` の `Option` に 3 つ目の
状態を足す。rc2 の NULL には一番近いが、`target_class` を読む所が `src/vm.rs` だけで 70 か所あり、
`heap.class(tc)` を直に呼ぶ所を全部書き換えることになる。`CallInfo` は命令ループで毎回写す型で、大きさも
変わる。隠れたモジュールなら、読み落とした所があっても、それは空のモジュールを読むだけで panic には
ならない（`ObjId(u32::MAX)` のような番兵は、読み落としが panic になるので選ばなかった）。

見えるようになった副作用: ヒープの生きた数が 1 つ増える。X2 の `gc_*` の本のプログラム（rc2 とは元から
違う）の SabiRuby 側の数が 630 → 631 などに動き、X1 の `gc-setter` の例の `GC.stat` の `live` も 1 増えた。
`ObjectSpace.count_objects` も T_MODULE を 1 つ多く数える（rc2 とは元から数が違う）。

本の `data/code/exp_TDEF_target_class.rb` を rc2 と SabiRuby で走らせ、出力が一致した。

### テスト

`tests/custom/` に 3 つ足した。期待値はどれも rc2 の答え（`tools/custom.sh` で `.expected` を `.rc.out` から
作り、SabiRuby の出力と一致することを見た）:

- `bytesplice_two_args.rb`: 2 つの引数の形 8 つ（数と数、数と nil、数と文字列、nil と文字列、Range と文字列、
  3 つの引数）。
- `setmcnst_frozen.rb`: 凍結したクラス・モジュール・特異クラスへの `A::Y = 2`、クラス本体の `X = 1`、凍結して
  いないクラス、`3::X = 1`。
- `def_no_target_class.rb`: 上の rc2 の確かめを 1 つにまとめたもの（`def`／`alias`／`undef`、Float・Symbol・
  `instance_exec`・文字列、中のブロック、後で呼ぶ proc と lambda、`class_eval(&pr)`、`eval` と
  `binding.eval`、定数・クラス変数・`class`・`Module.nesting`・`Module.constants`、`each_object` に隠れた
  モジュールが出ないこと）。

初めは `K.const_defined?(:W, false)` で定数の行き先を見ていたが、SabiRuby はこれが Object の定数にも true を
返すので（表に無い差。下）、`K.constants(false)` に替えた。`Object.private_method_defined?` は rc2 に無い
（NoMethodError）ので `respond_to?(m, true)` に替えた。

結果（全部 `9e6e44b` に今回の変更を足した木で）:

- `cargo test --release`: 全部通る。`custom: 63 passed, 0 pending, 0 problems`（60 + 3）。
- `cargo test`（debug）: 共有の `target/` では `tests/convert.rs` の 1 つが、ソースのどこにも無い文言
  （"no implicit conversion of nil into Integer"）で落ちた。古い成果物を見ていると判断し、別の
  `CARGO_TARGET_DIR` で回すと全部通った（変更の無い `9e6e44b` も別の target で通る）。debug でも
  `custom: 63 passed`。`const_class` の `debug_assert` に当たらないことはこれで見た。
- `tools/custom.sh`（全部）: 新しい 3 つは agrees。DIFFERS は 18 で、どれも `.expected` が rc2 と違うと決めて
  あるもの。`.mrb`・`.dump`・`.rc.out` に git の差は出なかった。
- `tools/mrbtest.sh`（3 ビルド）: utf8 2508 / ok 2346 / ko 51 / crash 100 / skip 11、bytes 2453 / 2269 / 38 /
  90 / 56、no-regexp 2012 / 1981 / 9 / 17 / 5。生成した md の差は日付だけだったので戻した（baseline と同じ）。
- `tools/check_no_std.sh`: no_std OK。`tests/send_sync.rs` 通る。VM の crate に `unsafe` は足していない。
- 本の 2 つのプログラム: 上のとおり、rc2 と一致。

### 洗い出しを回し直す

Y0 の手順のとおり。`target/rc2-sweep` を `target/rc2-sweep-y0` に移して、`lists`・`cases`（一覧と
`chunks.json`・`one_side.tsv`・`skipped.tsv` は Y0 とバイト単位で同じ）、`run --timeout 5 --only
'^(?!Range:endless$)'`（58 s、**restarts 0**。Y0 は 51 で、全部 bytesplice の panic だった）、`(1..)` の raw を
`target/rc2-sweep-y0/raw/{ref,sabi}/Range_endless.tsv` から写して `compare.py`、`programs.py run --timeout 60` と
`compare`（71 s）。`compare.py diff` の要点:

```
X1 calls compared / without an answer: 977521 / 4345 (same)
X1 causes: 92 -> 91; 0 new, 1 gone, 1 changed
  changed	bytesplice	A	4434->2175 (-2259)	0	1511->732 (-779)	2923->1443 (-1480)	0
  gone	crash	A	-51	-0	-51	-0	-0
X2 x2-programs.tsv: 502 -> 505 rows; 3 added, 0 removed, 15 changed
  added: custom/bytesplice_two_args.rb (same) / custom/def_no_target_class.rb (same) / custom/setmcnst_frozen.rb (same)
  changed: book/exp_GETCONST_frozen.rb, kit-samples/exp_GETCONST_frozen.rb: stdout -> same
  changed: book/exp_TDEF_target_class.rb, kit-samples/exp_TDEF_target_class.rb: stdout -> same
  changed: book/gc_{disabled,heap_pages,objspace,observe,stat}.rb と kit-samples の同じ 5 つ: 最初の差の行の SabiRuby の数が 1 増えた（differs のまま）
  changed: book/vm_task_sleep_switching.rb: sabi_out_len 297 -> 296（Y0 の揺れ）
X2 x2-asserts.tsv: 250 -> 250 rows; 0 added, 0 removed, 0 changed
```

`crash` が消え、`setmcnst-frozen` と `tdef-no-target`（X2 の原因で、手で書いた `x2-causes.tsv` に載っている）は
それぞれの本のプログラムが `same` になった。ほかの原因は増えていない。`gc_*` の 10 行は隠れたモジュールの 1 つ
（上）。`time-now` は今回は前と同じ 2 呼び出しで出た。生成した表（`x1-causes.tsv`、`x1-counts.md`、
`x1-methods.tsv`、`x2-programs.tsv`）はこの走行の結果でコミットする。README の要約と `x2-causes.tsv` は
書き換えていない（Y6 で回し直した結果と一緒に）。

### 性能

`tools/bench_ab.sh --core 2`（5 巡、A と B を交互）。A は `9e6e44b` を別の `CARGO_TARGET_DIR` で建てたもの
（md5 `ff5d518d…`）、B は今回のもの（`e32ad6a8…`）。A/A は A の写し 2 つ。同時に走らせたのは 1 つだけ。
隣で別の repo の Ruby のテストが動いていた（並行実行あり、`vmstat` の idle は 95〜100%）。

| 走行 | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---:|---:|---:|---:|---:|---:|
| A/B 1 回目 | -1.2% | -2.2% | -1.4% | +0.3% | +3.3% | **-0.9%** |
| A/A | +0.2% | -0.6% | -0.1% | +0.3% | +0.2% | **+0.1%** |
| A/B 2 回目 | -6.5% | +1.3% | -3.4% | -1.9% | +0.5% | **-4.3%** |

A/A は静かで、1 つずつのベンチの幅は -4.1%（`vmo_objects`）〜 +6.4%（`call_fiber`）、ほとんどは ±1% の中。
A/B の 2 回はどちらも騒がしく（best と median の差が 1% を大きく超える行が多い。絶対の時間が A/A より
10〜15% 遅い）、1 回目で遅く見えた `mem_short_lived` +7.6%・`vmo_objects` +8.0% は 2 回目で -0.7%・+0.4%、
2 回目で遅く見えた `ds_string` +8.0%・`loop_if_branch` +5.5% は 1 回目で -1.8%・-4.6%。2 回とも A/A の幅を
超えて遅いベンチは無く、合計は 2 回とも速い側。変更が触るのは `OP_SETMCNST` の腕（凍結の 1 回の読み）、
`TDEF`/`ALIAS`/`UNDEF`/`TCLASS` の腕（`ObjId` の比べ 1 回）、`proc_target_class`（比べ 1 回）で、どの文言も
冷たい関数（`frozen_error`、`no_target_error`、`bytesplice_two_args`）の中で組む。遅くなったとは読めない、と
判断した。表は `scratchpad` の `bench/y1-{ab,aa,ab2}.tsv`（repo には入れていない）。

### edge_compare

`tools/edge_compare.sh` で測り直した。ファイル 30 passed / 17 failed / 1 runtime_error、assert は pass 859 /
fail 32 / error 34 / skip 2 で、コミットしてある結果と同じ。生成物の差は時間・コミット・`dirty`・
メッセージの中のアドレス（`#<Class:0x…de00>` → `…de40`、隠れたモジュールの分ずれた）だけだったので戻した。

### 迷った点

- **`crash` の止め方を rc2 の答えまで進めた**: 計画は「panic を止めるだけ、rc2 と同じ答えは Y2」。panic して
  いた枝（引数 2 つ、最初が Range でない）の rc2 の答えは 2 通りしかなく、TypeError に近い形にしようとすると
  そのまま rc2 の答えになるので、その枝だけは rc2 と同じにした。文言の綴り "arumgnts" も計画の Y2 の方針
  どおり写した。ほかの形は触っていない。
- **「クラスが無い」の表し方**: 上の「選んだ形」。公開 API は変わらない（`no_target` は `pub(crate)`）。
- **`Setmcnst` の名前付けと凍結の順**: rc2 は名前を付けてから凍結を見る。`Setconst` に合わせ、凍結を先に
  見るままにした。

## 気づいた点（Y1）

- **SabiRuby の `Module#const_defined?(name, false)` が Object の定数にも true を返す**
  （`src/builtins/object.rs:156`、第 2 引数を見ずに `const_get` で探す）。rc2 は `class K; end;
  K.const_defined?(:String, false)` が false。洗い出しの表には無い（呼ぶ表が `false` を渡していないため
  と思う。確かめていない）。属する先: VM（A の候補）。
- **`"s".instance_eval { SW = 5 }` の定数の行き先**: rc2 は Object（`OP_SETCONST` は proc のクラスを読む）、
  SabiRuby は特異クラス（フレームのクラスを読む）。今回はフレームにクラスが無い場合だけ proc を読むように
  した。表には無い。属する先: VM（A の候補）。`eval` も同じく、SabiRuby はフレームのクラス、rc2 は proc の
  クラスを使う（`"s".instance_eval { eval("def x; end") }` の行き先が違うはず。確かめていない）。
- **凍結したクラスへの `A::B = Class.new` の名前**: rc2 は FrozenError の前に無名クラスに "A::B" と名前を
  付ける（`mrb_const_set` の `mrb_class_name_class` が `mrb_obj_iv_set` より先）。SabiRuby は付けない
  （`Setconst`・`Setmcnst` とも）。コードを読んだだけで、走らせて確かめていない。属する先: VM（小さい）。
- **`range_beg_len`（`src/builtins/ext_array.rs:134`）の `end += 1`** は終わりが `i64::MAX` の閉じた Range で
  溢れる。release は溢れの検査が無いので巻き戻って長さ 0 になる。debug ビルドでは panic しうる
  （上の 163,512 回の中では当たらなかった）。rc2 の `mrb_range_beg_len` も同じ足し算をする
  （C の符号付きの溢れ）。属する先: VM（`bytesplice` の外、多くのメソッドが通る）。
- **隠れたモジュールの分、ヒープの生きた数が 1 つ増える**（`GC.stat` の `live`、`ObjectSpace.count_objects`
  の T_MODULE、本の `gc_*` のプログラムの SabiRuby 側の数）。rc2 とは元から違う数。本が SabiRuby の数を
  引いていれば 1 ずれる。属する先: 本（確かめるなら本体）。
- **共有の `target/` の debug の成果物が古い**（`tests/convert.rs` が、ソースに無い文言で落ちる）。
  属する先: 道具／環境。

## Y2. A の残り（rc2 に合わせる）

基準: main `46e79f6`（Y1 の後）。コードのコミットは `0e84a6d`（原因が多く、同じファイルに複数の原因が
入るので 1 つにまとめた）。

### 進め方

前の走行の `target/rc2-sweep/x1-diffs.tsv`（差のあった呼び出し全部）から原因ごとに受け手・メソッド別の
例を抜き出す小さな道具（スクラッチパッドの `bycause.py`）と、その例を `__send__` の形に書き直して
1 つの `.rb` にまとめ、rc2（`kishima/mruby:4.1.0-rc2-utf8`）と SabiRuby で走らせて差だけを出す道具
（`gen.py` と `d.sh`、アドレスと一時ディレクトリのパスを消してから `diff`）を作った。原因ごとに rc2 の
ソース（`/home/kishima/book/ref/mruby`、HEAD `c17ffcc24` のまま）を読み、1 行の再現を両方で走らせて
答えを確かめてから写し、同じ道具で抜き出した呼び出しが全部同じになるまで直した。最後の一括の確かめでは
25 原因の 512 呼び出しのうち、下に書く残りの 32 行だけが違った。

### 原因ごと

- **`bytesplice`**: rc2 の `mrb_str_bytesplice`（src/string.c:4414 あたり）は引数の数で読み方を分ける。
  2 個は `"oS"`（Range と String）、3 個は最初が Integer なら `"iiS"`、それ以外は 2 つ目を
  `mrb_ensure_string_type` してから最初と 3 つ目を `mrb_range_beg_len(..., FALSE)`、5 個は `"iiSii"`。
  それ以外の数と、Range でない・範囲外の Range はどれも `default` の ArgumentError
  "wrong number of arumgnts"（綴りのまま）。範囲の検査と長さの切り詰めは `str_bytesplice` で、凍結の
  検査は書く直前（最後の空の範囲への追加は `mrb_str_cat` の中）。Y1 の 2 つの枝の止め方は全体の書き直し
  に吸収した（`str_bytesplice_m`、`range_arg`、`str_bytesplice`。`string.rs`）。SabiRuby は前は
  `argc!(2, 5)` で "wrong number of arguments (given 0, expected 2..5)" だった。
- **`reinitialize`**: `range_initialize` は `range_ptr_init` の `r_check`（"bad value for range"）が
  "'initialize' called twice" より先。`mrb_ary_init`（src/array.c:462）は、Array 1 つでブロックも
  2 つ目もなければ写し、そうでなければ `mrb_as_int` の大きさ → `ary_modify_check` → 0 から size-1 まで
  `mrb_ary_set` で上書き。消しもしないし、負の大きさも拒まない。だから `Array.new(-1)` は `[]` に
  なる（前は ArgumentError "negative array size"）。利用者に見える変化なので CHANGELOG に太字で書いた。
  方針（rc2 が例外を出さずに受けるものは受ける）で決まる。ブロック付きの `Array.new(n) { }` の
  ループ（`ary_init_step`）も、値を貯めて最後に入れ替える形から、1 つずつ受け手に書く形にした（rc2 の
  ループと同じ順。ブロックの途中で配列を覗くと見え方が変わる）。`mrb_str_init` は引数なしで "" に
  置き換える。`mrb_class_initialize`（"|C&"）と `mrb_mod_initialize` は self を返し、ブロックを
  そのクラスで走らせる。SabiRuby には Class/Module の `initialize` が無く BasicObject の nil を返して
  いたので足した。`Class.new` は前のまま `initialize` を呼ばない（rc2 は `initialize` が
  再定義されていれば呼ぶ。表に無いので触っていない）。"%v is not class/module" は `%v` が `to_s`
  なので nil は " is not class/module" になる（`not_class_module`）。Hash の凍結の検査は `hash_modify`
  が先（`mrb_hash_init`）。
- **`set-reinit`**: `Set#initialize` は Ruby（set.mrb）で、最初に `__init`（`set_init`）を呼ぶ。rc2 は
  `kset_is_uninitialized` でなければ RuntimeError "already initialized set"。SabiRuby の Set は Hash の
  形で「未初期化」の状態を持っていなかった。Set は Hash の既定値を使わないので、その `default` の枠を
  「初期化した」の印にした（`initialized`、`mark_initialized`。`dup` は `HashData` ごと写すので印も
  付いていく。`initialize_copy` でも付ける）。`set-uninit`（C）の「未初期化の Set が `|` を拒む」は
  今までどおり再現しない。
- **`argc-wide` / `argc-narrow`**: `instance_methods` と `Module.constants`（引数あり）は ANY で宣言され
  本体が "|b" なので、2 つ目は ArgumentError。`mrb_ary_sample` は OPT(2) で "|i?:" なので、最初の引数を
  変換してから数える。`mrb_data_initialize` は ":" だけなので、キーワードの不足（"missing keyword"）が
  数より先。この文言の違い（rc2 "missing keyword: x"、SabiRuby "missing keywords: :x, :y"）は B の
  `data-init` に移った（2 → 992 呼び出し）。Hash の `initialize(0) { }` は凍結が先（上）。
  `define_singleton_method(x, k: 1)` の 22 呼び出しは、rc2-leftovers10 で著者が決めた「2 つ目の引数に
  Proc を取る」の記録済みの差（キーワードの Hash も 2 つ目の引数として数えられる）なので直さず、
  `compare.py` の `known()` の規則を広げて k に移した。
- **`frozen`**: `mrb_ary_delete` は要素を動かすときにだけ `ary_modify` を呼ぶ。何も見つからなければ
  書かないので、凍結した配列でも nil（またはブロックの値）。rc2 は最後の要素だけが一致したときも凍結を
  見ずに長さを縮める（凍結したものが変わる、C の `frozen-set` と同じ種類の不具合）ので、見つかったときは
  今までどおり FrozenError にした。`mrb_str_chomp_bang` は "|S" の後で `mrb_str_modify_keep_cr`。
  "S" は nil を受けないので `chomp!(nil)` も `chomp(nil)` も TypeError（`lenient-arg` も同じ根）。
  `hash_merge`（mruby-hash-ext）は引数を 1 つずつ、型 → `mrb_hash_merge`（凍結の検査 → 写す）なので、
  凍結した受け手では最初の Hash で FrozenError、凍結していなければ前の Hash を写してから TypeError
  （`merge-partial` も同じ根）。`mrb_ary_set` と `mrb_ary_splice` は `ary_modify` が最初。
  残りは 32 呼び出し: `Set[1].freeze.replace(x)` の 30（rc2 の `replace` は Ruby の `clear` + `merge` で、
  凍結した Set を空にしてから ArgumentError。C の `frozen-set` と同じ不具合なので直さない）と、
  `[]=` の長さが `2**70` の 2（rc2 の `aget_index` はメソッドの最初の引数を変換する。README の記録済みの
  差で、SabiRuby は長さそのものを変換して RangeError）。
- **`fiber-inspect`**: `fiber_to_s` は終わっていない fiber について、文脈の最初のフレームの proc の
  pc 0 の位置（`mrb_debug_get_position`）を足す。SabiRuby では fiber の文脈が持つブロック
  （`Context::proc_`）、根の fiber なら最初のフレームの irep（`fiber_position`）。`&:to_s` の fiber は rc2 で
  CFUNC なので位置が無く、SabiRuby も同じく出ない（irep に位置が無い）ことを確かめた。
- **`join-empty`**: `join_ary` は各配列の 2 つ目以降の要素の前に区切りを置く。入れ子の空配列にも区切りは
  付く。SabiRuby は「最初に何か書いたか」で区切りを決めていた。再帰の検査も、rc2 は今いる道の上の
  全部の配列と比べるので、そう直した（前は根だけ）。`Enumerator#inspect` は Ruby で `join` を呼ぶので
  一緒に直る。
- **`method-missing-name`**: `mrb_obj_missing` は "n*!"。名前が Symbol でも String でもなければ
  TypeError、引数なしは "given 0, expected 1+"。NoMethodError の `args` に残りを入れる。
- **`binding-inspect`**: mruby-binding は `inspect` を `mrb_any_to_s` で定義している。足した。これで
  隠れていた `local_variable_get` などの文言の差（rc2 `:""`、SabiRuby `''`）が B の `msg-other` に
  出てきた（+44）。新しい原因ではなく、前は `binding-inspect` が先に取っていた。
- **`conv-order`**: `regexp_match` は "o|i&" で位置を先に変換し、そのあと nil の主語は nil
  （`regexp-match-nil` も同じ根）。`mrb_f_caller` は "|oi" なので長さが先。`mrb_bint_pow` は幅の広い
  指数を "too big power"。`mrb_bint_powm` は Integer でない法を `mrb_as_bint`（`mrb_as_int`）にかけるので
  NaN・Infinity は RangeError。`mrb_bint_and` などは Integer でない相手を `mrb_as_bint` にかけるので、
  幅の広い受け手と NaN・Infinity は RangeError。どちらも有限の Float は rc2 では切り捨てられて黙って
  違う値になる（`(2**70) & 1.5` が 0）ので、方針「mruby の限界は仕様」の後半により SabiRuby の TypeError
  のまま残した。ただし文言 "can't convert Float into Float" は意味をなさないので、小さい受け手の
  `bit_op` の拒み方（`mrb_int_noconv`）の "can't convert Float into Integer" にした。この 12 呼び出し
  （`&`、`|`、`^`、`allbits?`、`anybits?`、`nobits?` に 1.5）は表では `bigint-to-float` に残る。
  `Math.ldexp(2**70, 2**70)` は "fi" の f が通るようになって（下）、i の RangeError になった。
- **`visibility-args`**: `mrb_mod_visibility` と `mrb_mod_module_function` は名前を 1 つずつ型を見て
  適用する。
- **`bigint-to-float`**: `mrb_ensure_float_type`（src/object.c:715）は Integer・Float・Rational・Complex
  （虚部が 0 のとき）・幅の広い Integer を Float にする。SabiRuby の `Math` の `as_float` は小さい
  Integer と Float しか受けていなかった。`num_f64` と Complex の `to_f64` を通すようにして、
  `Float#remainder`（"f"）もこれにした。それ以外の値の文言は B の `conv-float-msg`（Y4）なので今の
  ままにした。`Complex.polar` も直った（中で Math を通す）。
- **`math-nan-domain`**: rc2 の検査は `x < -1.0 || x > 1.0` などを誤りとする形で、NaN はどの比較も
  偽なので通り、NaN を返す。SabiRuby は「通るもの」を `contains`・`>=` で書いていたので NaN が落ちて
  いた。rc2 の比較の否定に書き直した。
- **`proc-init-copy`**: `mrb_proc_init_copy` は Proc でなければ ArgumentError "not a proc"。Proc なら
  `mrb_proc_copy` が「すでに中身のある proc」は何もせず戻り、そのあと受け手に `MRB_PROC_ORPHAN` が
  付く。SabiRuby の Proc は必ず中身を持つので、印を付けるだけにした（最初は写す形で書き、rc2 では
  `pr.call` が元の値のままだったので直した）。
- **`time-year`**: `time_to_s` は C の `strftime` の `%Y`（埋めない）、`time_asctime` は `"%.4d"`
  （符号の後に 4 桁）。rc2 の像には `Time#strftime` が無い（SabiRuby の `strftime` は一方だけにある
  メソッドなので触っていない）。
- **`sum-block`**: rc2 の `sum` は mruby-enum-ext の `Enumerable#sum`（Ruby）だけで、Array には無い。
  SabiRuby には v0 から native の `Array#sum`（ブロックを見ない）があった。「gem が Ruby で定義する
  メソッドをネイティブで置き換えない」の約束にも反するので消した。持ち主も Enumerable になる（Y3 の
  持ち主の差の 1 つが先に消える）。
- **`rational-minus`**: `mrb_rational_sub` だけは `default` が `mrb_as_float`（`add`・`mul` は相手に
  渡す）。`ensure_f64`（rc2 の文言）を通した。
- **`enum-args-kw`**: `"abc".gsub(k: 1)` の中で SabiRuby の native が `to_enum` を `funcall` すると、
  最後の引数が呼び出し側のキーワードの Hash と同じなので `pending_kw` によりキーワードとして渡され、
  Ruby の `to_enum` が捨てていた。rc2 は C から値として渡す。`pending_kw` をその呼び出しの間だけ外した。
- **`negative-zero`**: `flodivmod` の `if (mod == 0) mod = 0.0`。`Float#%` と `Integer#%`（Float）に
  同じ 1 行を足した。
- **`split-empty`**: `mrb_str_split_m` は残りを足すのを `RSTRING_LEN(str) > 0 && (lim_p || len > beg ||
  lim < 0)` のときだけにする。空の受け手には何も足さない。同じ条件で、空白の split（awk）は末尾が
  空白のとき limit が負なら空の欄を足す（`" a\nb c ".split(nil, -1)` が `[..., ""]`）。これは表で
  `auto: v String#split`（1 呼び出し）だったもので、同じ条件の同じ関数なので一緒に直した。awk の
  ループは C のとおりに写した（`isspace` は `\v` を含むので Rust の `is_ascii_whitespace` は使わない）。
- **`string-add-redefine`**: `OP_MATH_CASE_STRING_add` は 2 つの String（どのクラスでも
  `MRB_TT_STRING`）を `mrb_str_plus` で足し、`+` を探さない。SabiRuby の `op_arith`（命令の腕の外の
  冷たい側）に String どうしの枝を足した。`String#+` の本体は `str_plus` に出して共用。整数と Float の
  速い道（`arith!`）には触っていない。本の `overview_diff.rb` の該当の行が rc2 と同じになった
  （X2 で `sabi_out_len` 1023 → 1009。プログラム全体は `require` と `eval-error-line` で違うまま）。
- **`int-div-nonfinite`**（著者の判断: 両方 rc2）: `int_idiv` は小さい受け手で割る数を `mrb_as_int` に
  かける。NaN・Infinity・幅の広い Integer はそこで RangeError。`ceildiv` は Ruby の `-div(-other)` なので
  一緒に直る（Infinity は `-other` で "-Infinity" の文言になるのも rc2 と同じ）。有限の Float は C の
  `int-div-float` で、床の割り算のまま。
- **`fill-empty-range`**（著者の判断: CRuby の答え）: `ary_fill_parse_arg` は `mrb_range_beg_len` の
  戻り値を真偽で見ていて、`MRB_RANGE_OUT`（0 でない）も通り、書かれていない `range_beg`・`range_end` を
  読む。そこだけ CRuby の `rb_range_beg_len(..., err = 1)` の答えにした（`fill_range_out`: 末尾より先の
  開始はそこから埋める、配列より前は RangeError "X out of range"）。rc2 が値を返す範囲（`[1, 2, 3].fill(0,
  1..5)` は rc2 で `[1, 0, 0]`、CRuby は伸ばして `[1, 0, 0, 0, 0, 0]`）は rc2 のまま。CRuby 3.2.6 で
  同じ 11 行を走らせ、その 1 行以外は一致した。C としての記録（README、`.rc.out` の扱い）は Y5。
- **Y1 の候補 `const_defined?(name, false)`**: `mrb_mod_const_defined` → `const_defined_0`。`inherit` なら
  親の鎖を全部（モジュールは Object に戻らない）、`false` なら受け手の表だけ、ただし受け手が Object なら
  鎖を上る。SabiRuby は第 2 引数を見ずに `const_get`（Object まで探す）だった。
- **Y1 の候補 `range_beg_len` の溢れ**: rc2 の順（`trunc` の切り詰め → 後ろから数える → 端を含めるなら
  +1、ただし `trunc` で `end < len` のときだけ）に並べ直した。切り詰めた端が +1 に届かないので
  `trunc` では溢れない。`trunc` なしで閉じた `i64::MAX` は rc2 でも C の符号付きの溢れで、
  `wrapping_add` にした（release の今までの答えと同じ長さ 0、debug でも panic しない）。
- **Y1 の候補 `"s".instance_eval { SW = 5 }`**: rc2 で確かめた。`OP_SETCONST` は
  `MRB_PROC_TARGET_CLASS(ci->proc)` に書くので、クラス本体の中の `"s".instance_eval { SW = 5 }`、
  `Object.new.instance_exec { }`、`String.class_eval { SU = 10 }` はどれもその本体のクラス（K）に入る
  （`class_eval` でも String ではない）。トップレベルでは Object。ブロックの中のブロックは、作られた
  フレームのクラス（`"s"` の特異クラス）に入る。SabiRuby の `Op::Setconst` を `const_class`（GETCONST が
  使う「proc のクラスを読むべきフレームでは読む」関数）にした。Y1 の「フレームにクラスが無いときだけ
  proc」の特例はこれに含まれる。

### 確かめの途中で見たこと

- rc2 の `eval` の文字列の定数には状態による漏れがある。トップレベルで `M.module_eval("CD = 4")` を
  した後は、トップレベルの `eval("CN = 5")` も `[1].each { CO = 1 }` も M に入る（新しいファイルで
  `Class.new { CH = 8 }` は Object に入るが、その後では入らない）。状態で答えが変わるので不具合として
  SabiRuby は追わない。`class K; binding.eval("CG = 7"); end` は rc2 で Object、SabiRuby で K（これは
  Y2 の前から）。
- `"s".instance_eval { eval("def zz; end") }` と `String.class_eval { eval("def yy; end") }` は、rc2 では
  どちらもブロックが書かれたクラス（K）に定義される。SabiRuby はフレームのクラス（特異クラス、String）。
  `String.class_eval { def yy; end }` は rc2 でも String に入るので、rc2 の中で食い違う（C の候補）。
  表に無いので直していない（下の気づいた点）。

### テスト

`tests/custom/` に 3 つ足した。

- `rc2_sweep_y2.rb`: 上の原因ごとの節（130 行ほど）。期待値は rc2（`tools/custom.sh` が `.rc.out` から
  作った `.expected`）。アドレスを Regexp を使わずに消す（regexp の無いビルドでも走るように。最初は
  `gsub(/0x\h+/)` で書いて no-regexp のビルドで落ちた）。
- `rc2_sweep_y2_regexp.rb`（`# regexp-only:`）: `Regexp#match` の変換の順と `gsub` の Enumerator。
- `fill_empty_range.rb`: `.expected` は手で作った（CRuby 3.2.6 の答え、範囲の中は rc2 の答え）。
  `.rc.out` は rc2 の未初期化の値で、`tools/custom.sh` は DIFFERS と言う（そう決めたもの）。

結果（`0e84a6d`）:

- `cargo test --release`: 全部通る。`custom: 66 passed, 0 pending, 0 problems`（63 + 3）。
  bytes のビルド（`--no-default-features --features std,regexp`）64 passed、no-regexp のビルド
  （`--features std,utf8`）59 passed（どちらも別の `CARGO_TARGET_DIR`）。
- `cargo test`（debug、別の `CARGO_TARGET_DIR`）: 全部通る（`const_class` の `debug_assert` を
  `SETCONST` が通るようになったので確かめた）。
- `tools/mrbtest.sh`（3 ビルド）: utf8 2508 / ok 2346 / ko 51 / crash 100 / skip 11、bytes 2453 / 2269 /
  38 / 90 / 56、no-regexp 2012 / 1981 / 9 / 17 / 5。baseline と同じ。生成した md の差は日付だけなので
  戻した。
- `tools/check_no_std.sh`: no_std OK。VM の crate に `unsafe` は足していない（`git diff` で確かめた）。

### 洗い出しを回し直す

Y1 と同じ手順（`target/rc2-sweep` を `target/rc2-sweep-y1` に移し、`lists`・`cases`・`run --timeout 5
--only '^(?!Range:endless$)'`（58 s、restarts 0）、`(1..)` の raw を Y1 の走行から写して `compare.py`、
`programs.py run --timeout 60` と `compare`）。`lists` の差は SabiRuby 側のメソッドの一覧だけ（`Array#sum`
が消え、`Binding#inspect`、`Class#initialize`、`Module#initialize`、`Proc#initialize_copy` が増えた。
どれも rc2 にあるもの）。`compare.py diff` の要点:

```
X1 calls compared / without an answer: 977521 / 4345 (same)
X1 causes: 91 -> 67; 1 new, 25 gone, 13 changed
  gone	argc-narrow, argc-wide, binding-inspect, bytesplice, conv-order, enum-args-kw, fiber-inspect,
        int-div-nonfinite, join-empty, lenient-arg, math-nan-domain, merge-partial, method-missing-name,
        negative-zero, proc-init-copy, rational-minus, regexp-match-nil, set-reinit, split-empty,
        sum-block, time-year, visibility-args, auto: v String#split (1), time-now (2, 揺れ),
        known: a NaN has no identity (3)
  changed	bigint-to-float	A	152->12      (& | ^ allbits? anybits? nobits? に 1.5: 残したもの)
  changed	frozen	A	1136->32               (Set#replace の 30 は C frozen-set、[]= の 2 は記録済み)
  changed	reinitialize	A	3970->748       (全部 (1..) の受け手: Y1 の raw を写したもので走らせていない)
  changed	argc-splat	C	12042->12834 (+792)  (chomp! の splat の呼び出しが frozen・argc-wide から移った)
  changed	data-init	B	2->992 (+990)        (Data#initialize が argc-narrow から移った。文言だけの差)
  changed	msg-other	B	26->70 (+44)          (Binding の local_variable_* の文言。binding-inspect が隠していた)
  changed	conv-int-msg	B	2781->1379, conv-float-msg 5648->5641, conv-msg-other 452->447
  changed	internal-helper	D	15178->15001, kw-dropped ? 706->694, float-ulp D 20->22
  new	known: define_singleton_method(name, a_proc) (README)	D	+22   (規則を広げたもの)
X2 x2-programs.tsv: 505 -> 508 rows; 3 added, 0 removed, 5 changed
  added: custom/fill_empty_range.rb (stdout), custom/rc2_sweep_y2.rb (same), custom/rc2_sweep_y2_regexp.rb (same)
  changed: book/overview_diff.rb と kit-samples の同じもの: sabi_out_len 1023 -> 1009（String の + の行）
  changed: book/gc_disabled.rb と kit-samples の同じもの: SabiRuby の数 3634 -> 3633（differs のまま）
  changed: book/vm_task_sleep_switching.rb: sabi_out_len 296 -> 300（Y0 の揺れ）
X2 x2-asserts.tsv: 250 -> 250 rows; 0 added, 0 removed, 0 changed
```

A の原因で残るのは `bigint-to-float`（12）、`frozen`（32）、`reinitialize`（748）の 3 つで、どれも上の
とおり直さないと決めたものか、走らせていない受け手。新しく出た原因は `known:` の規則を広げた 1 つだけ。
`float-ulp` の +2 は `Math.cbrt(2**70)`（幅の広い Integer が通るようになって初めて呼べた。libm の `cbrt`
と glibc の `cbrt` の最後の 1 桁）。`(1..)` の `initialize` は SabiRuby で `(1..).__send__(:initialize,
true, 0)` が "bad value for range" になることを手で確かめた。生成した表はこの走行の結果でコミットする。
README の要約と `x2-causes.tsv` は書き換えていない（Y6）。

### 性能（いったん止めた。下の「性能の切り分け」で直した）

`tools/bench_ab.sh --core 2`（5 巡、交互）。A は `9e6e44b`（Y1 の前、md5 `ff5d518d…`、Y1 のベンチと同じ
バイナリ）、B は `0e84a6d`（`1bc29cdb…`）。A/A は A の写し 2 つ。回す前に `docker ps` が空（mrubyedge の
container なし）、`pgrep -f bench_ab.sh` が空、`vmstat` の idle 99% を確かめた。同時に 1 つだけ。

| 走行 | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---:|---:|---:|---:|---:|---:|
| A/B 1 回目（騒がしい: best と median の差が大きい） | +4.0% | +0.2% | +5.9% | +2.0% | -0.3% | **+3.7%** |
| A/A（騒がしい。1 つずつは -10.4%〜+7.7%） | +2.9% | +0.9% | -3.8% | +2.0% | -2.8% | **+1.0%** |
| A/B 2 回目（静か: best と median がほぼ同じ） | +4.0% | +2.5% | +9.7% | +4.8% | +2.0% | **+5.1%** |
| Y1（`46e79f6`）/ Y2（`0e84a6d`）（静か） | +4.1% | +2.5% | +10.1% | +4.6% | +1.2% | **+5.1%** |

静かな 2 回がそろって +5.1%、命令ループの分類が +10% で、A/A の幅を超えている。遅いのは
`vmo_dispatch` +14%、`vmo_arith` +11%、`loop_if_branch`・`loop_while_add` +8.5%、`bm_so_mandelbrot`
+8%、`call_fiber`・`ds_array` +8% など、整数の足し算と分岐の多いもの。Y1 の上で測っても同じなので、
Y2 の変更による。計画の「A/A の幅を超えて遅くなったら止めて報告」に従い、ここで止めた。原因は
探っていない。候補（確かめていない）: Y2 で命令ループの関数（`src/vm.rs` の `exec`）に入った変更は
`Op::Setconst` の腕（`const_class` は `#[inline]` で、中の `debug_assert` は release では消えるが、
腕が大きくなる）と、`op_arith`（冷たい側）の String の枝だけで、`arith!` の速い道そのものは変えて
いない。命令ループの関数のコードの配置やレジスタの割り当てが変わった可能性がある。`Setconst` の腕を
`#[inline(never)]` の関数に出す、`op_arith` の枝を別の冷たい関数に出す、のどちらかを 1 つずつ戻して
A/B を取れば切り分けられる。表は `scratchpad` の `bench/y2-{ab,aa,ab2}.tsv`、`bench/y1-y2.tsv`。

`tools/edge_compare.sh` の再計測は、性能で止めたので回していなかった（直した後に回した。下）。

### 性能の切り分けと直し（本体の指示、著者の了承済み）

**候補 1: `Op::Setconst` の腕**。中身を `#[cold] #[inline(never)]` の `op_setconst` に出したもの（`v1`）を
`9e6e44b` と比べた（`vmo_` の 5 本だけ）: `vmo_dispatch` +14.5%、`vmo_arith` +9.5% で変わらず。
**候補 2: `op_arith` の String の枝**を `match` の腕のまま冷たい関数 `op_add_str(x, y)` に出したもの（`v1`
の上、`v12`）: `vmo_dispatch` +21.5% と、かえって悪い。どちらも効かないので、Y2 の差分を分けて建てた
（Y1 `46e79f6` の上に、別の worktree と `CARGO_TARGET_DIR` で）:

| 建てたもの | 中身 | `vmo_dispatch` | `vmo_arith` | `vmo_` 全体 |
|---|---|---:|---:|---:|
| `bi` | Y1 + `src/builtins` の Y2 の差分だけ | +0.4% | -0.0% | +0.1% |
| `bs` | `bi` + `Op::Setconst` の差分 | +0.4% | +0.1% | +1.1% |
| `v2` | `bs` + `op_arith` の枝を冷たい関数に（`match` の腕から呼ぶ） | +21.2% | +15.2% | +12.2% |
| `0e84a6d` | `bs` + `op_arith` の枝をそのまま | +14.2% | +10.6% | — |

（`bi`・`bs` の 2 回目、静かなとき。どれも A = Y1。）`op_arith` の差分が原因と分かった。ところが
`exec_frames` の機械語は `bs` と `0e84a6d` で番地の他は 1 命令も違わない（`objdump` の差が無い）。
`perf record` で `vmo_dispatch` を見ると、`0e84a6d` では時間の 12.5% が `op_arith` の先頭 0x50 バイト目
（2 つ目の被演算子を `%rsp` に書く `movapd`）にあった。`op_arith` は命令の腕の外の冷たい関数のつもり
だったが、`i += 1` は 4.1 のコンパイラでは `OP_ADDILV`（`Op::Addilv`）になり、その腕は速い道を
持たずに毎回 `op_arith` を呼ぶ（`src/vm.rs` の `Op::Addilv | Op::Subilv`）。`match` に String の枝
（ガードで `x`・`y` を参照し、関数に値で渡す）を足したことで、LLVM が 2 つの被演算子を `match` の前に
スタックに置くようになり、整数の道が毎回 32 バイトをメモリに書いて読む形になった（Y1 の `op_arith` は
レジスタに読んでいる）。全関数を 64 バイト境界にそろえた建て方（`-C llvm-args=-align-all-functions=6`、
Y1 と Y2 の両方）でも `vmo_dispatch` +15.9% が残ったので、置き場所ではない。

**直し**（`4fc10d1`）: String の枝を `match` から外し、数の場合が全部済んだ後の `None` の道（`op_send`
の直前）で `mid == plus` のときだけ、`#[cold] #[inline(never)]` の `op_add_str(r)` を呼ぶ。被演算子は
渡さず、関数がレジスタ（`self.stack[r]`、`[r + 1]`）から読み直す（`x`・`y` を値で渡す形では、まだ
スタックに置かれた。`objdump` で `op_arith` の先頭がレジスタに読む形に戻ったことを見た）。答えは
同じ（String どうしの `+` を探さずに足す）。

確かめ（`4fc10d1`）: `compare.py diff`（Y2 の表に対して）は X1 が `no change`、X2 は揺れの
`vm_task_sleep_switching.rb` の長さだけ（生成した表は戻した）。mrbtest（3 ビルド）は baseline と同じ、
custom 66 / 64（bytes）/ 59（no-regexp）、`cargo test --release` 全部通る、`check_no_std.sh` OK。

**ベンチ**（A = `9e6e44b`、B = `4fc10d1`、`--core 2`、5 巡。`docker ps` に mrubyedge は無かったが、
別の repo の fmruby の container が 5 つ動いていて、割り込みが毎秒 2 万ほどあった。並行実行あり。
A の合計が 45 秒前後の回は静か、55 秒を超える回は騒がしい）:

| 走行 | A の合計 | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---:|---:|---:|---:|---:|---:|---:|
| A/B 1（静か） | 44.8 s | +0.9% | +1.6% | +0.5% | +3.5% | +0.9% | **+1.2%** |
| A/A 1 | 49.8 s | +2.9% | +0.9% | -3.8% | +2.0% | -2.8% | **+1.0%**（Y2 の最初の A/A） |
| A/A 2 | 56.6 s | -4.7% | -1.7% | -4.5% | -1.1% | +6.5% | **-3.4%** |
| A/B 2（騒がしい） | 58.7 s | -1.7% | +0.3% | +1.5% | +1.3% | +5.0% | **-0.2%** |
| A/B 3（騒がしい） | 55.8 s | -7.8% | +4.1% | +0.7% | +1.7% | +2.0% | **-3.5%** |
| A/A 3 | 51.3 s | -0.5% | -0.5% | -0.5% | +0.6% | -0.6% | **-0.3%** |
| A/B 4（静か） | 46.0 s | -0.1% | +1.0% | +1.4% | +2.5% | +0.2% | **+0.7%** |
| A/A 4 | 47.3 s | +0.8% | +8.7% | -1.2% | -2.8% | -5.0% | **+0.2%** |

命令ループは +10% から +0.5〜+1.4% に戻り、`vmo_dispatch` は +0.4%。合計は静かな 2 回で +1.2% と
+0.7% で、A/A の幅（-3.4%〜+1.0%）にほぼ入る。ただ `calls` の分類は A/B の 4 回とも +1.3〜+3.5% と
正の側にそろっていて（A/A は -2.8〜+2.0%）、1 本ずつでは `vmo_calls` が 4 回とも +0.9〜+4.8%（A/A は
-2.1〜+0.4%）だった。Y1 と Y2 のどちらの分かは、機械が騒がしくなって分けられなかった（Y1/Y2 の
`call_` だけの比べは 1 本ずつ ±10% に揺れた）。指示の終わりの条件（合計が A/A の幅）は満たしたと
判断して止めた。表は `scratchpad` の `bench/fix-*.tsv`、`bi-vmo2`・`bs-vmo2`・`v2-vmo`・`al-*`。

本の素材になりそうなこと: 「冷たい」つもりの関数でも、ある命令（ここでは `OP_ADDILV`）の唯一の道に
なっていれば熱い。そこに枝を 1 つ足すと、関数全体のレジスタの割り当てが変わり、熱い整数の道が
メモリを経由するようになる。命令ループの関数の機械語が 1 命令も変わっていなくても起きる。

### edge_compare（`4fc10d1`）

`tools/edge_compare.sh`: ファイル 30 passed / 17 failed / 1 runtime_error（前と同じ）、assert は pass 858 /
fail 33 / error 34 / skip 2（前は 859 / 32）、`assert.rb` の参考の列は ok 883 / ko 43（前は 884 / 42）。
減った 1 つは mruby/edge の `string.rb` の「String#+ redefined on String itself reaches the
redefinition」で、`string-add-redefine` を rc2 に合わせた結果（rc2 も `"a" + "b"` で再定義を呼ばない）。
`string.rb` は前から failed のファイル。生成物はこの結果でコミットした。

### 迷った点

- **`Array.new(-1)` が `[]` になる**: rc2 の `mrb_ary_init` は負の大きさを拒まない（ループが回らない
  だけ）。方針（rc2 が受けるものは受ける）で決まると判断したが、利用者に見える変化なので CHANGELOG で
  太字にした。
- **有限の Float を黙って切り捨てる所**（`(2**70) & 1.5`、`7.div(1.5)`、`(-3).pow(2**70, 1.5)`）は
  方針「mruby の限界は仕様」の後半（黙って違う値は不具合）で SabiRuby のまま。`&` などの文言は
  "can't convert Float into Float" が意味をなさないので、小さい受け手と同じ "can't convert Float into
  Integer" にした（B の文言の領分に少し入る）。C の記録（Y5）に足すかは本体の判断。
- **凍結した配列の `delete` で最後の要素だけが一致したとき**、rc2 は凍結を見ずに縮める。凍結したものが
  変わる不具合（C の `frozen-set` と同じ種類）なので FrozenError のまま。Y5 の記録に足す候補。
- **`split` の awk の末尾の空の欄**は表では `auto`（1 呼び出し）だったが、`split-empty` と同じ条件
  （string.c:3399）なので一緒に直した。
- **`define_singleton_method(x, k: 1)`** は記録済みの差として `compare.py` の `known()` を広げた（道具の
  変更）。
- コミットは原因ごとに分けず 1 つにした（同じファイル・同じ関数に複数の原因が入るため）。

### 気づいた点（Y2）

- **性能**: Y2 で命令ループが約 10% 遅かった（`op_arith` の String の枝。`4fc10d1` で直した）。`vmo_calls` に
  +1〜5% が残っている可能性がある（上）。属する先: VM。
- **`Op::Addilv`/`Subilv` に速い道が無い**（`i += 1` が毎回 `op_arith` を呼ぶ）。`Op::Addi` のように整数の
  速い道を腕に持たせれば速くなるはず（測っていない）。属する先: VM（最適化の候補）。
- **`Math.cbrt` の最後の桁**: `Math.cbrt(2**70)` が rc2 `10568983.798516534`、SabiRuby
  `10568983.798516532`（libm と glibc）。Float の引数でも同じはず（確かめていない）。表では `float-ulp`
  （D）に入った。属する先: VM（D、libm の精度）。
- **`Binding#local_variable_*` の文言**: rc2 は名前を `%!n`（`:""`、`:abc`）、SabiRuby は `''`。
  `binding-inspect` が隠していたもので、B の `msg-other` に入った（+44）。属する先: Y4。
- **`Time#initialize` の失敗の後の受け手**: rc2 は `Time.at(0).utc.__send__(:initialize, 0, 0)` が
  ArgumentError の後、受け手の `inspect` も ArgumentError になる（壊れた Time が残る）。SabiRuby は元の
  まま。`auto: v Time#initialize`（22）。rc2 の不具合の候補。属する先: 本家／C の候補。
- **rc2 の `Time.gm(-12345)` が `-12346-12-31`**（`Time.local` は正しい）。表には無い（値の範囲の外）。
  属する先: 本家の不具合の候補。
- **rc2 の `eval` の文字列の定数の漏れ**（上の「確かめの途中で見たこと」）。トップレベルの
  `module_eval` の文字列の後、トップレベルの定数が全部そのモジュールに入る。属する先: 本家の不具合の
  候補（本で `eval` の定数に触れていれば）。
- **`eval("def …")` をブロックの中で呼んだときの行き先**: rc2 はブロックの書かれたクラス、SabiRuby は
  フレームのクラス（`String.class_eval { eval("def yy; end") }` が rc2 では外側のクラスに入る）。
  表に無い差。属する先: VM（A か C かは著者）。
- **`class K; binding.eval("CG = 7"); end`** は rc2 で Object、SabiRuby で K（Y2 の前から）。表に無い。
  属する先: VM（同上）。
- **`Class.new` は `initialize` を呼ばない**（rc2 は再定義されていれば呼ぶ）。表に無い。属する先: VM。
- **`(1..)` の受け手**は今回も走らせていない（`reinitialize` の 748 はその古い raw）。手で 1 つ確かめた。
  属する先: 道具。

## Y3. メソッドの持ち主を rc2 に合わせる

基準: main `6ef09f7`（コミットは main の先の `cc10e6c` の上。間の 2 つは計画書と backlog だけ）。

### 数える（着手の前）

`x1-one-side.tsv` は「片方の `instance_methods(false)` にだけある」の表で、持ち主の違いと本当に片方にしか
ない差が混ざっている（`Numeric#<` は「SabiRuby only」、`Integer#<` は「rc2 only」に別々に出る）。そこで
持ち主を直接比べる小さな Ruby（スクラッチパッドの `y3/owners.rb`）を両方で走らせた: `methods.rb` と同じ
やり方で Object から届くクラス・モジュールを集め、それぞれについて `public/protected/private_instance_methods(true)`
の全部を `instance_method(x).owner` 付きで、`singleton_methods(true)` を `method(x).owner` 付きで書き出す。
両方にあって持ち主だけが違う行を `cmp.py` で集めた。片方にしか無いものはこの比べ方では出ない（範囲外）。
もう一つ、`sweep.py lists` が書く `receivers.*.tsv`（受け手ごとの呼べるメソッドと持ち主）も比べた。前者は
クラスの側から、後者は受け手（特異メソッドを含む）の側から見る。

結果（`6ef09f7`、`sweep.py lists` を取り直したもの）: クラス × メソッドで 146 行、(メソッド, rc2 の持ち主,
SabiRuby の持ち主) で 52 組、SabiRuby の定義で数えて **42 個**。受け手の側からはこれに特異メソッドの
`Range.new` と `Module.new`（rc2 は `Class#new`）の 2 個が加わり、**合わせて 44 個**。受け手の側の
`Kernel` の `` ` `` も持ち主が違って見えるが、rc2 のそれは mruby-io の `` Kernel.` ``（特異メソッド）で、
SabiRuby の private な `Kernel#`` ` は `Kernel.` で呼べない。片方にしか無い差（V1-1 の「意図した差」）
なので数えていない。内訳（SabiRuby の定義 → rc2 の持ち主）:

| 群 | SabiRuby の定義 | rc2 の持ち主 | 数 |
|---|---|---|---:|
| 数の比較 | `Numeric#<` `<=` `>` `>=` | Integer・Float は自分、Numeric・Rational・Complex は Comparable（compar.rb） | 4 |
| | `Numeric#<=>` | Integer・Float は自分、Numeric は Kernel | 1 |
| | `Numeric#==`、`Numeric#between?` | Comparable | 2 |
| | `Numeric#finite?` `infinite?` | Float は自分（Integer は Numeric のまま） | 2 |
| | `Integer#-@` `Float#-@` | Numeric（mrblib numeric.rb） | 2 |
| | `Integer#eql?` `Float#eql?` `Rational#eql?`、Numeric 自身は `Kernel#eql?` | Numeric（`num_eql`） | 4 |
| Enumerable | `Array#take` `drop` `hash`、`Hash#to_a` `hash` | Enumerable（enum.rb、mruby-enum-ext） | 5 |
| Kernel | `Array#dup` `freeze` `frozen?`、`Hash#dup` `freeze` `frozen?`、`String#dup` `freeze` `frozen?` `===`、`Range#dup`、`Proc#dup` `clone`、`Symbol#===` `hash` | Kernel | 15 |
| | `Module#initialize_copy` | Kernel | 1 |
| BasicObject | `Exception#==`、`Module#==`、`Proc#initialize`、`Kernel#__id__`（BasicObject にもあった） | BasicObject | 4 |
| 自分のもの | `Kernel#dup`（Module）、`Kernel#nil?`（NilClass） | `Module#dup`、`NilClass#nil?` | 2 |
| 特異メソッド | `Range.new`、`Module.new` | `Class#new` | 2 |
| | | | **44** |

**移すと消えるもの**: 無かった。移した後の `receivers.sabi.tsv` を前と比べると、どの受け手でも消えた
メソッドも増えたメソッドも 0 で、変わったのは持ち主と、Ruby になったものの引数の名前（`between?` の
`[[:req, :min], [:req, :max]]` など。rc2 と同じになった）だけ。

### 移す

**Integer・Float・Numeric**（`numeric.rs`）: rc2 の `numeric_rom_entries`・`integer_rom_entries`・
`float_rom_entries`（src/numeric.c:2471-2566）に合わせた。比較の 5 つは名前付きの関数（`num_cmp_m`、`num_lt`
など）にして Integer と Float の表から同じものを指す。Numeric の表には `eql?`（`num_eql` を写したもの）と
`-@`、元からの `finite?`・`infinite?`・`fdiv` が残る。`cmp` の「受け手がオブジェクトなら `<=>` を送る」枝
（Rational・Complex が Numeric の `<` を通っていたときのもの）は、受け手が Integer か Float だけになって
通らなくなったので消した。`Numeric#infinite?` が Integer で nil を返すのは README の記録済みの差なので
そのまま（rc2 は false）。

**Comparable の Ruby を使うか、ネイティブを移すか**: `between?`、Rational・Complex の `<` などは、rc2 では
mrblib compar.rb の Ruby で、SabiRuby も同じ compar.rb を `core.mrb` に持っている（`tools/fixtures.sh` が
本家の mrblib をそのまま mrbc にかけたもの）。ネイティブの `Numeric#between?` などは、それを上書きしていた
だけなので、消せば rc2 と同じ Ruby が呼ばれる。Comparable にネイティブを置くと String など Comparable を
使うすべてのクラスで本家の Ruby を置き換えることになり、文言や `<=>` の呼び方を全部写す必要がある。
ベンチで `between?` や Rational の比較を使うものは無い（`bench/src` を grep）。だから Ruby の定義にした。

`Numeric#-@` は性能で決めた（下の「性能」）。rc2 では mrblib numeric.rb の `def -@; -1 * self; end` で、
Integer・Float の `-@` を消すとこれが呼ばれる。`-v` が Ruby のメソッドのフレームになり（`-v` を 1,000 回
回すプログラムで VM の命令が 10,738 → 15,738、1 回あたり 5 命令）、`bm_ao_render`（`-v` を多用する）が
+3.1% で A/A の幅（±1.2%）を超えた。ネイティブの `Numeric#-@` を Numeric の表に書いた版も測ったが、
変わらなかった。表に書いたネイティブは、後から読み込む `core.mrb` の Ruby の定義に上書きされていて、
走っていたのは Ruby の方だった（`--stats` の命令の数が Ruby の版と同じだったことで気づいた）。
Integer・Float の表にあった前のネイティブは、Numeric の Ruby を覆っていただけだった。そこで
`string::post_mrblib` と同じく、mrblib を読んだ後にネイティブの `Numeric#-@` を登録する
（`numeric::post_mrblib`）。持ち主は rc2 と同じ Numeric のまま、Integer と Float は直接
（`-1.0 * f`。rc2 では `OP_MUL` が送らずに計算する所）、それ以外は `-1` に `*` を送る（Ruby と同じ道）。
残る違いはバックトレースに `-@` の Ruby のフレームが出ないことだけ。これで 27 本のベンチ全部で、VM の
実行した命令の数が `6ef09f7` と 1 つも違わなくなった（`run --stats` で 1 本ずつ比べた）。

**Kernel に移したもの**: 特化した `dup`・`freeze`・`frozen?`・`===`・`hash` を消した。`Kernel#dup`
（`object.rs`）は Proc を写さず self を返していたので、`proc_dup` の中身をその腕に移した（写しは
`MRB_PROC_ORPHAN`、そのあと `initialize_copy` が呼ばれるのは rc2 と同じ）。`Proc.new` は「`initialize` の
持ち主が Proc でなければ呼ぶ」を「BasicObject でなければ」に直した（rc2 は必ず送るが、BasicObject の
ものなら何もしない）。

最初は `Kernel#dup` のまま Array・String の中身を写していたが、そのあと `initialize_copy` がもう一度写す
（Array は `ensure_array` と `slots_of` でさらに 2 回）。rc2 の `init_copy`（src/class.c）は
MRB_TT_ARRAY・MRB_TT_STRING の中身を写さず、`initialize_copy`（`mrb_ary_replace_m`、`mrb_str_replace`）に
任せる。そこで Array と String は空で作るようにし、`Array#initialize_copy` も相手が普通の Array なら
slots を 1 回だけ写すようにした（`bm_so_lists` の `li1.dup`。下の「性能」）。答えが変わるのは `initialize_copy` を `super` なしで
上書きしたときで、rc2 と同じ空の写しになる（テストで確かめた）。Struct・Data（Array の形）の
`initialize_copy` は自分で `ary_replace` するので変わらない。Hash は ivar と `HashData` を写したまま
（ベンチに出ないので触っていない）。

**BasicObject・自分のもの**: `Exception#==`（クラスと文言を比べていた）を消すと rc2 と同じく同一性になる。
これで mrbtest の `exception.rb` の `Exception#exception [15.2.22.5.1]` が 1 つ落ちた: SabiRuby の
`Exception#exception(e)` は受け手自身を渡されても写しを返していて、`Exception#==` が写しを「同じ」に
見せていた。rc2 の `exc_exception`（src/error.c:87）は `mrb_obj_equal(self, a)` なら self を返すので、
その 1 行を足した（rc2 は写しに `mrb_obj_clone` を使い、SabiRuby は `dup`。凍結した例外の扱いが違うはず
だが確かめていない。気づいた点）。`Module#==` と `Module#initialize_copy` は消し（`Kernel#dup` のクラスの
腕は `initialize_copy` を呼ばないので、写し方は変わらない）、`Module#dup` は rc2 の `mrb_mod_dup` どおり
`clone` してから凍結を外す。`NilClass#nil?` は true を返す。`Kernel#__id__` は消した（BasicObject のものが残る）。

**`Kernel#<=>` の再帰**: Numeric の `<=>` を消すと `Numeric.new <=> Numeric.new` が SystemStackError に
なった。`Kernel#<=>` が `==` を送り、Numeric の `==` は Comparable の Ruby で `<=>` を送るので、終わらない。
前は Numeric のネイティブの `<=>` が答えていて隠れていた。rc2 の `mrb_cmp_m`（src/kernel.c:168）は、
外側に同じ受け手・同じ引数の `<=>` のフレームがあれば nil を返す。SabiRuby のネイティブはフレームを
持たないので、最初に `method_recursive`（フレームを見る）と同じ形で書いた版は効かなかった（`caller` に
出る `in <=>` は呼び出しの命令から読んだ名前で、フレームではない）。`Kernel#<=>` が走っている間の
（受け手, 引数）を `Vm::cmp_guard` に積み、それとフレーム（Ruby の `<=>`）の両方を見るようにした。
rc2 の確かめ: `Numeric.new <=> Numeric.new` は nil、`include Comparable` だけのクラスの `==` は false、
`def <=>(o); [:sup, super]; end` は `c <=> c` でも `[:sup, nil]`（rc2 は C のフレームの呼び手が `<=>` なので
最初から nil。SabiRuby もフレームを見るので同じ）。

**`Range.new` と `Module.new`**: どちらも特異メソッドを消して `Class#new` に任せた。Range は
`instance_alloc` が既に「中身の無いオブジェクト」を作り、private の `Range#initialize` が埋める。Module は
`instance_alloc` に Module の形が無く（Object の形になる）、`InstanceKind::Module` を足して空のモジュールを
作るようにした。Class は前と同じ形のまま（`InstanceKind::Object` を明示。`Class.new` は自分の特異メソッド）。
これで `class MyMod < Module; end; MyMod.new` が MyMod になり（前は Module）、`Module.new(1)` が rc2 と同じ
ArgumentError に、Range の子クラスの `initialize` が呼ばれるようになった。`Module.allocate` を使うと
panic していたのも直った（`object ObjId(..) is not a class`。`6ef09f7` でも panic することを確かめた）。

### 確かめ

- 持ち主: `owners.rb` の比べで差が 0。受け手の側（`receivers.*.tsv`）で残るのは上の `` Kernel.` `` だけ。
- 本: `data/code/corelib_kernel.rb` の 3 行目が `[Kernel, Kernel, Enumerable, Range, Comparable]` で、本
  （corelib.re:137 の list、349 の「Comparable は … `between?` が compar.rb」）と一致した。2 行目の
  `Kernel.instance_methods(false).size` は 47 → 46（`__id__` の分）で、rc2 の 44 との差の 2 は
  `require`・`load`（片方にしか無いもの、X2 の別の原因）。corelib.re:158 の「NilClass は … `nil?` と `=~` を
  加えた 7 個」の `nil?` も、`NilClass.instance_methods(false)` が rc2 と同じ 13 個（mruby-object-ext 込み）に
  なった。
- `tests/custom/method_owner.rb`（期待値は rc2 の `.rc.out` そのまま）: 持ち主、`instance_methods(false)`、
  `super` の行き先（Integer に足した `between?` の `super`、Comparable を上書きしたとき）、Ruby になった
  ものの答えと文言、`Kernel#<=>` の再帰、`Exception#==`、`dup` と `initialize_copy`、`Module.new`・`Range.new`。
  途中で `class Array; def initialize_copy …; end` と `remove_method` で試す形を書いたが、`remove_method` で
  組み込みの `initialize_copy` まで消えて後の行に響くので、子クラスで試す形に直した。
- `cargo test --release`: 全部通る。custom 67（66 + 1）。bytes のビルド 65、no-regexp のビルド 60（別の
  `CARGO_TARGET_DIR`）。debug（別の target）も全部通る。`tests/send_sync.rs` 通る。
- `tools/mrbtest.sh`（3 ビルド）: utf8 2508 / ok 2346 / ko 51 / crash 100 / skip 11、bytes 2453 / 2269 / 38 /
  90 / 56、no-regexp 2012 / 1981 / 9 / 17 / 5。baseline と同じ（`exception.rb` の 1 つは上の直しの後）。
  生成した md の差は日付だけなので戻した。
- `tools/check_no_std.sh`: no_std OK。VM の crate に `unsafe` は足していない。

### 洗い出しを回し直す

Y2 と同じ手順（`target/rc2-sweep` を消して `lists`・`cases`・`run --timeout 5 --only '^(?!Range:endless$)'`
（49 s、restarts 0）、`(1..)` の raw は Y2 の走行（`target/rc2-sweep-y2`）から写して `compare.py`、
`programs.py run --timeout 60` と `compare`）。`compare.py diff`:

```
X1 calls compared / without an answer: 977521 / 4345 (same)
X1 causes: 67 -> 67; 0 new, 0 gone, 1 changed
  changed	known: hash values are SabiRuby's own (README)	D	90->84 (-6)	0	0	0	90->84 (-6)
X1 methods: 230 -> 230 rows; 2 added, removed or changed
  Array#hash: differ 8 -> 6 (k), Hash#hash: differ 8 -> 4 (k)
X2 x2-programs.tsv: 508 -> 509 rows; 1 added, 0 removed, 5 changed
  added: custom/method_owner.rb (same)
  changed: book/corelib_kernel.rb と kit-samples の同じもの: line 2 の SabiRuby の数 47 -> 46
  changed: book/exp_SCLASS_types.rb と kit-samples の同じもの: line 16 の NilClass の一覧に nil? が入った
           （並びは前から違う。differs のまま）
  changed: book/gc_observe.rb と kit-samples の同じもの: SabiRuby の生きた数 621 -> 620（differs のまま）
  changed: book/vm_task_dormant.rb と kit-samples の同じもの: SabiRuby の `GC.stat[:live]` の差 2 -> 1
           （rc2 の像には Task が無く、rc2 側は空。`6ef09f7` と比べて毎回この値）
  changed: book/vm_task_sleep_switching.rb: sabi_out_len 300 -> 295（Y0 の揺れ）
X2 x2-asserts.tsv: 250 -> 250 rows; 0 added, 0 removed, 0 changed
```

（上は最後のコードでの走行。`-@` を post_mrblib にする前の走行では `gc_observe`・`vm_task_dormant` の 2 行が
出ず、ほかは同じだった。どちらも rc2 とは元から違う SabiRuby 側の生きたオブジェクトの数で、答えの差ではない。）

`x1-one-side.tsv` から持ち主だけの差の 54 行（44 個の定義の両側）が消え、増えた行は無い。新しい原因は
出ていない。`hash` の k の減りは、Array と Hash の `hash` が rc2 と同じ `Enumerable#hash` になって値まで
一致した呼び出し。X2 の `method-owner`（手で書いた `x2-causes.tsv` の行）は `corelib_kernel.rb` の 3 行目
だが、表は「最初に違う行」しか持たないので 2 行目（別の原因）のまま。README の要約と `x2-causes.tsv` は
書き換えていない（Y6）。生成した表はこの走行の結果でコミットする。

### edge_compare

`tools/edge_compare.sh`: ファイル 30 passed / 17 failed / 1 runtime_error、assert は pass 858 / fail 33 /
error 34 / skip 2、`assert.rb` の参考の列は ok 883 / ko 43。前と同じなので、生成物（コミット・日時の差だけ）は
戻した。

### 性能

`tools/bench_ab.sh --core 2`（5 巡、交互）。A は `6ef09f7` を別の worktree と `CARGO_TARGET_DIR` で建てたもの
（md5 `1e7d6043…`）、A/A はその写し 2 つ。回す前に `pgrep -f bench_ab.sh` が空、`docker ps` に重い container が
無いことを見た。同時に 1 つだけ。途中で別の担当の container（playwright、fmruby、asterism）が立ち上がって
騒がしくなった回（A の合計が 47.9 s、best と median の差が大きい）は捨てた。

| 走行 | B | A の合計 | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| A/B 1 | 最初の版（`-@` は Ruby） | 44.1 s | +1.2% | +0.0% | +0.1% | -0.2% | -0.6% | **+0.6%** |
| A/A 1 | | 44.2 s | -0.4% | +0.2% | +0.1% | +0.2% | +0.6% | **-0.2%** |
| A/B 3 | 表に書いたネイティブ `-@`（実は Ruby が走っていた）、Array・String を空で写す | 46.9 s | +0.4% | +0.7% | -0.1% | -0.1% | -0.4% | **+0.2%** |
| A/A 3 | | 46.6 s | +0.5% | -0.1% | -0.2% | -0.1% | -0.1% | **+0.2%** |
| A/B 4 | 最後の版（`-@` を post_mrblib、`initialize_copy` を 1 回の写しに） | 46.5 s | -0.1% | -1.8% | +0.6% | -2.8% | +0.8% | **-0.4%** |
| A/A 4 | | 46.4 s | -0.2% | -0.4% | -0.1% | +0.3% | +0.5% | **-0.1%** |

命令ループの分類は A/A と同じ幅（-0.2〜+0.6%）。数の演算子は `OP_ADD`・`OP_LT` などの速い道
（`cmp!`、`arith!`、`op_compare`）を通り、メソッドの表を見ないので、持ち主を移しても変わらないはずで、
27 本のベンチ全部で VM の実行した命令の数が `6ef09f7` と同じ（`run --stats`）、`vmo_arith`・`vmo_dispatch`
の差も A/A の幅の中だった。

1 本ずつでは、最初の版で `bm_ao_render` +3.1%（A/A ±1.2%）、`bm_so_lists` +1.3〜2.2% が幅を超えた。
`bm_ao_render` は上の `-@`（Ruby のフレーム）で、`run --stats` の命令の数の差（472,066,695 → 478,911,880、
`ARGV` 8 の小さい走行で 6,752,909 → 6,850,574）から、Ruby の数行のプログラムで数を比べて突き止めた
（`-v` の 1,000 回で +5,000 命令。`x ^ ((x & 0xfffff) << 11)` や多重代入では差が無い）。`bm_so_lists` は
`li1.dup` が `Kernel#dup` になって、1 万要素の配列を 3 回写すようになったため（上）。直した後の
9 巡の `bm_` だけの比べ: `bm_ao_render` +1.3%、`bm_fib` +0.5%、`bm_so_lists` -0.6%、`bm_mandel_term` -1.0%、
`bm_so_mandelbrot` -0.4%（A/A の 9 巡では ±0.5%）。`bm_ao_render` の +1.0〜1.3% は 2 回の測りで残ったが、
VM の命令の数は A と同じで、部分のマイクロベンチ（`-v` 300 万回の 2 つの式、`Vec.new` に当たる
`V.new(1.0, 2.0, 3.0)` 200 万回）は A と同じか速い（0.37 s 対 0.37 s、0.66 s 対 0.65 s）。`perf record
-e cpu-clock` の上位の番地の並びと割合も A と同じで、番地が 0x4c0 ほどずれているだけだった。コードの
配置による差と判断して、ここで追うのをやめた（切り分けの途中の版: 表の変更だけ `x1` +1.2%、
＋`Kernel#<=>`・`Vm` の欄・`InstanceKind` `x2` +2.2%。どちらも `-@` が Ruby だったときのもの）。

表は `scratchpad` の `y3/bench/y3-{ab1,aa1,ab2,aa2,ab3,aa3,ab4,aa4,bm-ab,bm-aa,bm-ab4}.tsv`（repo には入れていない）。

### 迷った点

- **数え方**: `x1-one-side.tsv` は持ち主の違いと片方にしか無いものが混ざっているので、「両方で呼べて持ち主が
  違う」を基準にした。この基準では `Exception#==`（SabiRuby の表にだけある）も `Range.new`・`Module.new`
  （SabiRuby の特異メソッド、rc2 は `Class#new`）も持ち主の違いに入る。どれも移すと答えが rc2 に寄るので
  移した。`` Kernel.` `` は mruby-io の特異メソッドで、SabiRuby 側では呼べないので片方にしか無いもの（V1-1
  の意図した差）として外した。
- **Ruby かネイティブか**: Comparable・Enumerable の Ruby（`between?`、Rational の `<`、`take`・`drop`・
  `hash`・`to_a`）は rc2 と同じ Ruby にした。ベンチに出ず、本家の Ruby を写し直すより確か。`Numeric#-@`
  だけは `bm_ao_render` が幅を超えたので、mrblib の後にネイティブを登録した（持ち主は rc2 と同じ Numeric）。
- **移したら出てきた差を直したもの**: `Kernel#<=>` の再帰の検査（Numeric の `<=>` が隠していた）、
  `Exception#exception(self)`（`Exception#==` が隠していた）、`Kernel#dup` の Proc の写し、Array・String の
  写し方（rc2 の `init_copy` と同じに）、`Proc.new` の `initialize` の判定、`InstanceKind::Module`。どれも
  持ち主を移すために要るもので、移した結果を rc2 に合わせた。
- **公開 API**: `InstanceKind` に `Module` を足した（前例の書き方で CHANGELOG に書いた）。
- **backlog**: 持ち主の差の行は backlog に無かった（26 行目の「数の 4 クラスの外で rc2 に無いメソッド」は
  片方にしか無いもので、Y3 の範囲ではない）。消した行は無い。

## 気づいた点（Y3）

- **`Class.allocate.new` が panic する**（`object ObjId(..) is not a class`、`src/object.rs:839`。`6ef09f7` でも）。
  `Class.allocate` が Class の形でない普通のオブジェクトを返し、`new` がそれをクラスとして読む。rc2 は未初期化の
  クラスを作り、`new` は例外。Y3 では Class の形を前のまま（`InstanceKind::Object`）にした。表に無い。
  属する先: VM（危ないもの）。
- **`Range.allocate.inspect`**: rc2 は ArgumentError "uninitialized range"、SabiRuby は `".."`。表に無い。
  属する先: VM（A の候補）。
- **`Exception#exception(msg)` の写し方**: rc2 は `mrb_obj_clone`（凍結と特異クラスも写す）、SabiRuby は
  `dup`。凍結した例外で違うはず。確かめていない。属する先: VM。
- **ネイティブのメソッドの `parameters`**: `receivers.*.tsv` で、rc2 の C 関数は `[[:opt]]` など、SabiRuby の
  ネイティブは `[[:req]]` など、多くの行で違う。前からの差で、洗い出しの原因の表では扱っていない。属する先:
  VM（`Method#parameters`）。
- **`x2-causes.tsv` の `method-owner`**: `corelib_kernel.rb` の 3 行目は一致したが、X2 の表は最初に違う行
  （2 行目の `Kernel.instance_methods(false).size`、46 対 44。`require`・`load` の分）しか持たないので、
  表の上では見えない。Y6 で表を書き直すときに。属する先: 道具／Y6。
- **`Kernel#dup` の Hash** は `HashData` を写したあと `initialize_copy`（`replace`）でもう一度写す。rc2 は
  `init_copy` で ivar だけを写す。ベンチには出ない。属する先: VM（小さい最適化）。
- **本**: corelib.re:137 の list の 2 行目（`Kernel.instance_methods(false).size` が 44）は SabiRuby では 46
  （前は 47）。本は rc2 の出力なので変える必要は無い。属する先: 本（参考）。


## Y4. B（文言を rc2 に合わせる）と、Y3 で見つかった表に無い差

基準: main `11e595c`（Y3 の後、計画書の更新まで）。コード・表・この節を 1 つのコミットにした（原因が同じ関数に
重なるため）。性能で止めた（下の「性能」）。

### 進め方

Y2 の道具（スクラッチパッドの `y2/bycause.py`、`y2/gen.py`、`y2/d.sh`）をそのまま使った。前の走行
（`target/rc2-sweep/x1-diffs.tsv`、`11e595c` のもの）から B の 11 原因の呼び出しを受け手・メソッドごとに 3 つずつ
抜き出して 1 つの `.rb`（707 行）にし、rc2（`kishima/mruby:4.1.0-rc2-utf8`）と SabiRuby で走らせて差だけを見た。
最初は 707 行すべてが違った。直すたびに回して、最後は 40 ずつ（5,767 行）でも違うのは 2 行だけになった（下の
`stack-error-gc`）。rc2 のソースは `/home/kishima/book/ref/mruby`（HEAD `c17ffcc24` のまま、読むだけ）。

途中で 1 度、`cargo build --release` だけでは `target/release/sabiruby` が建たないことに引っかかった（ルートの
パッケージはライブラリで、コマンドは `-p sabiruby-cli`）。最初の比べの 707 行は 13:09 に建った `11e595c` の
バイナリでのもので、基準としては正しい。

### 文言の作り（`conv-float-msg`、`conv-int-msg`、`conv-msg-other`）

計画書は `convert.rs` を挙げていたが、そこは `define_fn` の型の変換（`FromRuby`）で、数のメソッドの文言は
`src/builtins/numeric.rs` の `coerce_fail`（"can't convert X into Float"、`_op` を受けて使っていない）と
`float_binop` の "X can't be coerced into Float"、`ext_math.rs` の `as_float`、`ext_sprintf.rs` の `as_float`、
`ext_complex.rs`・`ext_rational.rs` のそれぞれの書き方が出していた。rc2 の側は 3 つの関数だけだった。

- `mrb_ensure_float_type`（src/object.c:715、`mrb_as_float` の中身）: nil は "can't convert nil into Float"、
  Integer・Float・Rational・Complex（`mrb_complex_to_f`、虚部が 0 でなければ RangeError）・幅の広い Integer は
  Float に、それ以外は "%Y cannot be converted to Float"。
- `mrb_int_noconv`（src/numeric.c:51）: "can't convert %Y into Integer"。
- `mrb_ensure_integer_type`（src/object.c:625、`mrb_as_int`）: "%Y cannot be converted to Integer"。

どのメソッドがどれを通るかは src/numeric.c を読んで決めた。小さい受け手の `Integer#+`・`-`（`mrb_int_add`／
`sub` の `default:`）、`%`（`int_mod` → `flodivmod`）、`divmod`（`flo_divmod(mrb_ensure_float_type(x))`）、
`fdiv`、`quo` は `mrb_as_float`。`*`（`mrb_int_mul`）、`/`（`int_div` の `default:`、受け手の幅によらない）、
`&`・`|`・`^`（`bit_op`）は `mrb_int_noconv`。`div`（`int_idiv`）と `**`（`mrb_int_pow`）は `mrb_as_int`。幅の広い
受け手は `mrb_bint_add` などが `mrb_as_bint` → `mrb_as_int` なので "%Y cannot be converted to Integer"、ただし
`mrb_bint_pow` だけは自分の文言 "%Y cannot be convert to integer"（綴りのまま。方針「文言は本家に合わせる」）。
Float の演算子（`flo_add` など）、`flo_pow`、`flo_div`、`flo_mod`、`flo_divmod`、mruby-math の `get_float_arg` と
`mrb_get_args("f")`、mruby-sprintf の `FMT_FLOAT`、mruby-complex の部分と相手、mruby-rational の `rational_new` の
Float の腕はすべて `mrb_as_float`。

そこで SabiRuby 側は `numeric::ensure_f64` を `mrb_ensure_float_type` の写しにし（Complex を足した。`num_f64` は
Complex を見ない）、`int_noconv` を足して `coerce_fail` を消し、呼んでいた所を rc2 の関数どおりに振り分けた。
`ext_math::as_float`、`ext_sprintf::as_float`、`ext_rational` の `opnd_f64`、`ext_complex` の 3 か所、`FromRuby for f64`
は `ensure_f64` を呼ぶだけにした。値が変わる所が 3 つ出た（どれも rc2 と同じになった）。`1.5 % Complex(2, 0)`
と `Math.sqrt(Complex(4, 0))` は `mrb_as_float` が Complex を受けるので値を返す。`1.div(Complex(2, 0))` は
`mrb_as_int` が受けて 0。`int_idiv` の小さい受け手は、Float・Rational（C の `int-div-float` として床の割り算の
まま）以外を `as_int` に通すようにした。

`bit` の幅の広い受け手で、相手が Rational か Complex のときは rc2 は `mrb_as_int` で切り捨てる（`(2**70) &
Rational(1, 2)` が 0）。Y2 の Float と同じく黙って違う値なので、TypeError のまま、文言だけ `mrb_int_noconv` の形
（"can't convert Rational into Integer"）にした。値でない相手（nil など）は rc2 どおり "nil cannot be converted to
Integer"。`Integer#pow(n, m)` の幅の広い道の法も同じ線で、値でないものだけ `mrb_as_int` の文言にした。

mruby-rational は `rat_type_error` が "%T cannot be converted to Rational"（nil でもクラス名）。`mrb_rational_div` は
受け手が `mrb_int` の Rational で割る数が Integer の 0 のときだけ `mrb_int_zerodiv`（"divided by 0"）、それ以外は
`rat_zerodiv`（"divided by 0 in rational"）。`Rational(2**70, "a")` は `rational_new_b(mrb_as_bint(a), b)` で b が
`mrb_as_int` を通るので "String cannot be converted to Integer"（数なら SabiRuby の Float の腕のまま。切り捨てを
避ける）。`Rational(1, 2) ** (2**70)` は `rat_pow_int` → `mrb_int_pow` で、分子が小さければ `mrb_as_int` の
RangeError "integer out of range"、幅が広ければ `mrb_bint_pow` の "too big power"（前は SabiRuby の
"exponent too large"。`msg-other` の 4 呼び出し）。

### ほかの B の原因

- **`module-required`**: rc2 の `include`・`prepend`・`extend`（src/class.c:2290, 2327, 2387）は各引数を
  `mrb_check_type(MRB_TT_MODULE)` で見る。`include?` は "C" のあとに同じ検査（2421）。`is_a?`・`kind_of?`・
  `instance_of?` は "c"（src/kernel.c:466, 503）。"c"・"C" は `ensure_class_type`（src/class.c:1231）の
  "%v is not class/module"。SabiRuby の `class_arg` を "c" の写しにし（"class or module required" は rc2 では
  `mrb_obj_is_kind_of` の到達しない枝）、`module_arg`（`mrb_check_type` の MODULE）を足した。値も 2 つ変わった:
  `include String` の文言がクラス自身でなく "Class"（`mrb_check_type` はオブジェクトのクラス名）、`include?(String)`
  が false から TypeError。
- **`class-new-super`**: `mrb_class_new_class`（src/class.c:3277）は "|C&" のあと `mrb_class_new` →
  `mrb_check_inheritable`（3532）: TT_CLASS でなければ "superclass must be a Class (%C given)"、Class 自身なら
  "can't make subclass of Class"。SabiRuby は `Class.new(Class)` も `class X < Class` も作っていた（値の差、表に
  無い）。同じ関数なので両方に入れた（`check_inheritable`、`OP_CLASS` の新しいクラスを作る枝）。
- **`type-nan-name`**: `mrb_check_type`（src/object.c:485）は nil・Integer・Symbol 以外の即値を `to_s` で、
  オブジェクトをクラス名で書く。rc2 の既定のビルド（64 ビットの word boxing）では Float の多くが即値で、NaN、
  非正規数、指数が 768..=1279 の外のもの、回転が番兵に重なるもの（`2.0**-255` など）がヒープに行く
  （`mrb_word_boxing_float_value`、src/etc.c:310）。`1.5` と `Infinity` は "1.5"・"Infinity"、NaN と `1e100` は
  "Float"。rc2 で 16 個の Float を `default_proc=` に渡して確かめ、`numeric::float_is_immediate` にその規則を写した
  （定数は rc2 の符号化そのもので選んだ数ではない。コメントに出どころ）。`ext_array::wrong_type`（SabiRuby の
  `mrb_check_type`）がそれを見る。`Hash#default_proc=` も `mrb_check_type(MRB_TT_PROC)`（src/hash.c:1714）なので
  それにした（`conv-msg-other` の 12）。`define_method` は "%T"（src/class.c:4178）で "NilClass"。
- **`not-a-symbol`** と `msg-other` の Binding: mruby-binding は "n"（`mrb_obj_to_sym`、"%!v is not a symbol nor
  a string"）、名前の誤りは "%!n"（Symbol の inspect、`:""`）、未定義は "local variable %!n is not defined"。
  `object::sym_arg` と `symbol::sym_inspect` を使った。
- **`unknown-keyword`**、**`data-init`**: `mrb_get_args` の ":"（src/class.c:1860-1901）は、必須の名前を表の順に
  見て最初の欠けで "missing keyword: %n"、そのあと残った最初のキーで "unknown keyword: %v"（`to_s`、`k`）。
  `OP_KEYEND`（src/vm.c:3861）も "%v"。`Vm::unknown_keyword` を足して、`OP_KEYEND`、`Array#sample`／`shuffle`
  （ext_random）、`Data`、`Task.new` から呼ぶ。`ext_data::take_keywords` は rc2 の順（欠けが先、未知が後）に
  書き直した。
- **`struct-const-name`**: `make_struct`（mruby-struct）の "identifier %v needs to be constant" は String の
  `to_s`（引用符なし）。
- **`eval-error-line`**（X2）: 2 つの半分がある。stderr の方は、rc2 の eval が `capture_errors` を立て、互換層
  （mruby_compat.c:42）がそれを `quiet_errors` として渡すので `codegen_error` が stderr に書かない。compiler
  crate の `csrc/shim.c` の eval の入口で `quiet_errors = 1` にした（vendor は触っていない）。行の方は、rc2 の
  SyntaxError の行は最初の診断の行（`copy_diagnostics_to_parser`、生成の誤りは 0、構文の誤りは文字列の中の行で、
  `eval` の行の引数は足さない: `eval("1 +", nil, "f.rb", 10)` が rc2 で "line 1"）。SabiRuby の VM は
  `Host::compile` から `Err(String)` しか受けないので、診断の行を知る道が無い。`Host` を変えるか（`Err` に行を
  持たせる、または既定の実装つきのメソッドを足す）は公開 API の名前と形の判断なので、直さずに止めた（迷った点）。
- **`msg-other` の `(1..3).lazy.sort`**（2 呼び出し）: rc2 の SystemStackError は 1 つの前もって作った例外で、
  GC が回るとその文言を消す（`clear_error_object`、src/gc.c:1484）。`def f; f; end; f` は rc2 でも "stack level too
  deep"。GC の時機で変わる文言なので不具合とみて SabiRuby のまま（README の表に `stack-error-gc` として足した）。

### 本の引用

`book_mruby3/data/code/corelib_error_messages.rb` を rc2 と SabiRuby で走らせ、全行が一致した（前は 6 行目、
corelib.re:442 の "TypeError: String cannot be converted to Float" が違った）。extension.re:474 の
"String cannot be converted to Float" は C の拡張の `mrb_get_args("ff")` に String を渡した出力で、SabiRuby で
これに当たるのは `define_fn` の `f64` の引数（`FromRuby for f64`）。`tests/convert.rs` の期待値を
"Symbol cannot be converted to Float" にした。

### Y3 で見つかった表に無い差

- **`Class.allocate.new` の panic**: rc2 の `Class.allocate` は TT_CLASS のオブジェクトで、`MRB_INSTANCE_TT` が 0、
  superclass が無い。`new`・`allocate` は `mrb_instance_alloc` が TT_OBJECT で頼み、`mrb_obj_alloc`
  （src/gc.c:866）が型の食い違いで "allocation failure of %C"。SabiRuby は Class の `instance_kind` が `Object` で、
  普通のオブジェクトを返していた。`InstanceKind::Class`（中身の無いクラスを作る）と `InstanceKind::Unset`（その
  クラスのインスタンスは作れない）を足した。rc2 と 32 通りの操作（`dup`、`clone`、特異クラス、`class_eval` の
  `def`、`const_set`、`include`、`Class.new(c)`、`class Foo < Class.allocate` など）を比べて全部同じ。
- **初期化していない Range**: rc2 は `mrb_range_ptr`（src/range.c:486）が RANGE_INITIALIZED を見て
  ArgumentError "uninitialized range"、Range のすべてのメソッドと、Range を引数に受ける所（`mrb_range_p` の後の
  `mrb_range_beg_len`）がこれを通る。SabiRuby の未初期化の Range は `ObjKind::Object` なので、Range のメソッドは
  nil の両端として答え（`".."`）、引数としては Range と見られず "Range cannot be converted to Integer" だった。
  `range::parts` を `mrb_range_ptr` の写しにし（Result を返す）、`range::is_range_obj`（`mrb_range_p`: 中身の有無に
  よらず Range のクラスのもの）を足して、Range を引数に受ける所（`range_beg_len`、`Array#[]`・`[]=`・`fill`、
  `String#[]`・`[]=`・`byteslice`・`slice!`、`MatchData`、`caller`、`rand`、`Random`）をそれにした。Range の
  すべてのメソッド（rc2 の `Range.instance_methods` を `Range.allocate` に送る小さなプログラムで比べ、違うのは
  一覧の並びの 3 行だけ）と 24 通りの引数の使い方が rc2 と同じ。`Range#==` は rc2 の `range_eq` どおり
  「同じクラスのインスタンス」（`mrb_obj_is_instance_of`）にしたので、`(1..2) == 子クラスの Range` は false に
  なった。`initialize_copy` も rc2 の順（元の中身を読む → `r_check` → 二度目の検査）にした。
- **`Exception#exception(msg)`**: rc2 の `exc_exception`（src/error.c:87）は `mrb_obj_clone` のあと
  `mrb_exc_mesg_set`（String でなければ `to_s`、凍結を見ずに書く）。clone にしたので特異メソッドと凍結が付いて
  いく（`object::obj_clone` を出した）。`e.exception(nil).message` は "" になった（前はクラス名）。残る違いは
  1 つ: rc2 の写しは backtrace を持たない。rc2 の `init_copy`（src/class.c:4349）は例外の ivar しか写さず、
  `RException` の文言と backtrace を写さないので、`RuntimeError.new("a").dup.message` も rc2 では
  "RuntimeError" になる。写しが黙って中身を失うので不具合とみて SabiRuby は写したまま（README の `exc-copy`、
  著者の判断待ち）。

### テスト

`tests/custom/rc2_sweep_y4.rb`（期待値は rc2 の `.rc.out` そのまま）: 原因ごとの節と、`Class.allocate`、
未初期化の Range、`Exception#exception`。最初に `(2**70) & Rational(1, 2)` を入れていて、rc2 の 0（切り捨て）と
違ったので外した（上の線どおり SabiRuby は TypeError）。

### 確かめ

- `cargo test --release --workspace`: 276 通る（0 失敗）。custom 68（67 + 1）、bytes のビルド
  （`--no-default-features --features std,regexp`）66、no-regexp のビルド（`--features std,utf8`）61（どちらも
  別の `CARGO_TARGET_DIR`）。
- `cargo test --workspace`（debug、別の target）: 276 通る。
- `tools/mrbtest.sh`（3 ビルド）: utf8 2508 / ok 2346 / ko 51 / crash 100 / skip 11、bytes 2453 / 2269 / 38 / 90 /
  56、no-regexp 2012 / 1981 / 9 / 17 / 5。baseline と同じ。生成した md の差は日付だけなので戻した。
- `tools/check_no_std.sh`: no_std OK。VM の crate に `unsafe` は足していない（`git diff` の追加行に 0）。

### 洗い出しを回し直す

Y3 と同じ手順（`target/rc2-sweep` を `target/rc2-sweep-y3` に移し、`lists`・`cases`・`run --timeout 5 --only
'^(?!Range:endless$)'`（43 s、restarts 0）、`(1..)` の raw は Y3 の走行から写して `compare.py`、`programs.py run
--timeout 60` と `compare`）。`compare.py diff`:

```
X1 calls compared / without an answer: 977521 / 4345 (same)
X1 causes: 67 -> 57; 0 new, 10 gone, 3 changed
  gone	class-new-super, conv-float-msg, conv-int-msg, conv-msg-other, data-init, module-required,
        not-a-symbol, struct-const-name, type-nan-name, unknown-keyword
  changed	msg-other	B	70->2 (-68)      (残りは stack-error-gc の 2)
  changed	internal-helper	D	15001->14997 (-4)
  changed	kw-dropped	?	694->691 (-3)
X1 methods: 230 -> 144 rows
X2 x2-programs.tsv: 509 -> 510 rows; 1 added, 0 removed, 5 changed
  added: custom/rc2_sweep_y4.rb (same)
  changed: book/corelib_error_messages.rb と kit-samples の同じもの: stdout が same に
  changed: book/overview_diff.rb と kit-samples の同じもの: stderr の差が消えた（stdout は require と eval の行で違うまま）
  changed: book/vm_task_sleep_switching.rb: sabi_out_len 295 -> 296（Y0 の揺れ）
X2 x2-asserts.tsv: 250 -> 250 rows; 0 added, 0 removed, 0 changed
```

新しい原因は無い。B の原因で残るのは `msg-other` の 2（`stack-error-gc`）と X2 の `eval-error-line`（行の半分）。
メソッドの表で主な原因が B から移った行（`Integer#fdiv` の `auto: e rc2 raise ZeroDivisionError` など）は、前から
あった差が B の後ろに隠れていたもの。生成した表はこの走行の結果でコミットする。README の要約と `x2-causes.tsv`
は書き換えていない（Y6）。表に無い差を 2 行（`exc-copy`、`stack-error-gc`）README の「? — the author decides」に
足し、`eval-error-line` の行に Y4 で直した半分と残りを書いた。

### edge_compare

`tools/edge_compare.sh`: ファイル 30 passed / 17 failed / 1 runtime_error、assert は pass 858 / fail 33 / error 34 /
skip 2、`assert.rb` の参考の列は ok 883 / ko 43。前と同じ（生成物の差はアドレスと時間だけ）なので戻した。

### 性能（止めた）

`tools/bench_ab.sh --core 2`（5 巡、交互）。A は `11e595c` を別の worktree と `CARGO_TARGET_DIR` で建てたもの
（md5 `56c61da6…`）、B は作業中の木（`c7378027…`）、A/A は A の写し 2 つ。回す前に `pgrep` に他の走行が無く、
`docker ps` が空、`vmstat` の idle 99% を見た。同時に 1 つだけ。

| 走行 | A の合計 | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---:|---:|---:|---:|---:|---:|---:|
| A/B 1 | 51.0 s | -0.6% | +1.2% | +1.9% | -0.5% | -1.2% | **+0.0%** |
| A/A 1 | 47.3 s | +0.1% | +0.0% | +1.1% | -0.5% | -0.2% | **+0.2%** |
| A/B 2 | 51.8 s | +1.8% | -1.2% | +3.5% | -0.1% | -1.5% | **+1.5%** |

合計は騒がしい回（A の合計が 51 s 台）でも A/A の幅に近いが、1 本ずつでは `bm_so_mandelbrot` が 2 回とも
+6.9%・+8.2%（A/A -1.0%）で、命令ループの分類も正の側にそろった。9 巡で絞って測った:

| 9 巡 | `bm_so_mandelbrot` | `loop_if_branch` | `loop_times` | `loop_while_add` |
|---|---:|---:|---:|---:|
| A/B | +7.6% | +7.9% | +4.7% | +2.9% |
| A/A | -2.1% | -0.4% | -1.2% | +0.6% |

A/A の幅を超えているので、計画と指示に従ってここで止めた。直してはいない。切り分けだけした: 命令ループの関数
（`exec_frames`）の大きさが A の 0xccda から B で 0xcc16 に変わっていた。Y4 で `exec_frames` の中に入った変更は
2 つ（`Op::Keyend` の文言の組み立てを冷たい `Vm::unknown_keyword` の呼び出しにしたこと、`Op::Class` の新しい
クラスを作る枝に `Class` の検査を 1 行足したこと）。B からこの 2 つだけを戻したもの（`v1`、0xccda に戻る）は
`bm_so_mandelbrot` +0.8%、`loop_if_branch` -0.2% で、A と変わらなかった。片方ずつ戻したもの（`v2`: `Keyend`
だけ戻す、0xcd06。`v3`: `Class` の検査だけ外す、0xcc89）は +1.5%／+2.1% と -8.3%／+0.1% で、この時は A の
`bm_so_mandelbrot` が 1283〜1509 ms と揺れていて決められなかった。Y2 と同じく、命令ループの関数の中のどこかが
変わると配置が変わって熱い腕が遅くなる形に見える（`exec_frames` の数の演算の腕そのものは変えていない）。
直し方の候補（試していない）: 2 つの変更を `exec_frames` の外に出す（`Keyend` は前の形の文言の組み立てを
`exec_frames` の中に残して文言だけ変える、`Class` の検査は新しいクラスを作る関数の側に置く）。表は
スクラッチパッドの `y4/{ab1,aa1,ab2}.log` と 9 巡の出力（repo には入れていない）。

### 迷った点

- **`eval-error-line` の行**: 上のとおり `Host::compile` の `Err(String)` では診断の行を返せない。案は (a)
  `Err` を行と文言の組（新しい型）にする（破壊的。rubevy の `EmbeddedHost`・`FileHost` も変わる）、(b) 既定の実装
  つきのメソッド（例: 行と文言を返す `compile_eval`）を `Host` に足し、VM はそれを呼ぶ（壊さない。名前と形は
  著者）、(c) 今のまま記録する。名前と公開 API の形の判断なので決めていない。stderr の半分だけ直した。
- **黙って違う値になる所**（`(2**70) & Rational(1, 2)`、`Rational(2**70, 1.5)`、幅の広い法の `pow` に Rational）
  は Y2 の線（方針「mruby の限界は仕様」の後半）で SabiRuby の TypeError か Float の腕のまま、値でない相手だけ
  rc2 の文言にした。
- **値が変わったもの**: `include?(クラス)` が TypeError、`Class.new(Class)` と `class X < Class` が TypeError、
  `1.div(Complex(2, 0))` などが Complex を受ける、`Range#==` が別のクラスの Range に false、未初期化の Range を
  引数に受けると ArgumentError。どれも文言の元の関数を写した結果で rc2 と同じ。CHANGELOG に書いた。
- **公開 API**: `InstanceKind` に `Class` と `Unset` を足した（Y3 の `Module` と同じ書き方で CHANGELOG に）。名前は
  rc2 の `MRB_TT_CLASS` と「`MRB_INSTANCE_TT` が 0」から取った。`FromRuby for f64` が Rational と実数の Complex を
  受けるようになった（`mrb_get_args("f")` の写し）。
- **`exc-copy` と `stack-error-gc`**: どちらも C に見えるが、決めずに表の「?」に足した。

## 気づいた点（Y4）

- **性能**: 上の「性能」。`exec_frames` の 2 つの腕の変更で `bm_so_mandelbrot` と `loop_*` が 3〜8% 遅い。
  属する先: VM。
- **`Integer#fdiv(0)`**: rc2 の `int_fdiv` は割る数が 0 なら ZeroDivisionError、SabiRuby は Infinity・NaN。表では
  `auto: e rc2 raise ZeroDivisionError`（8 呼び出し）で、B の後ろから見えるようになった。属する先: VM（A の候補）。
- **`Class.new` は `initialize` を呼ばない**（rc2 は再定義されていれば呼ぶ、src/class.c:3288）。Y2 の気づいた点と
  同じで、`mrb_class_new_class` を読み直して確かめた。属する先: VM。
- **`exc-copy`**: 例外の `dup`・`clone` が rc2 では文言と backtrace を失う。洗い出しの受け手に例外が無いので表に
  出ていなかった。属する先: 著者（C か A か）。
- **`mrb_check_type` を通るほかの所**: mruby-binding（INTEGER、PROC、ENV）、mruby-method（PROC、SYMBOL）、
  mruby-set など。表に差の出ていない所は `wrong_type` に揃えていない。属する先: VM（表に出たら）。
- **`Kernel#Rational` の `auto: e rc2 raise RangeError: float V out of range / sabiruby raise FloatDomainError`**
  （6 呼び出し）も B の後ろから見えるようになった。属する先: VM（A の候補）。

## Y4 の続き（著者の判断 2 つ、方針で決めた 2 つ）

基準: `feaa871`（Y4）。本体からの指示（著者の判断: 命令ループの 2 つの変更を外に出す、`eval-error-line` は
案 (b)。方針で決めたもの: `Integer#fdiv(0)` と `Kernel#Rational` の例外を rc2 に、`exc-copy` と `stack-error-gc` を
C に）。

### 命令ループの外に出す

`Op::Keyend` の文言と `Op::Class` の Class の検査を、Y2 の `op_add_str` と同じ形の `#[cold] #[inline(never)]` の
関数にした。オペランドはレジスタから読み直す: `op_keyend_unknown(ki)` は `stack[ki]` のキーワードの Hash の最初の
キーを読み、`op_class_inheritable(base + a + 1)` はスーパークラスのレジスタを見る（新しいクラスを作る枝で、
モジュールでないときだけ呼ぶ）。腕に残るのは「Hash が空でないか」の判定（A と同じ）と、呼び出し 1 つ。
`exec_frames` の大きさは 0xcbf1（A は 0xccda、Y4 の B は 0xcc16）。9 巡で絞った最初の確かめ（A = `11e595c`）:
`bm_so_mandelbrot` -0.0%、`loop_if_branch` +1.6%、`loop_times` +1.7%、`loop_while_add` +1.6%（同じ時の A/A は
前回 -2.1〜+0.6%）。全体の A/B と A/A は下の「性能」。

### `eval-error-line` の行: `Host::compile_eval`

`Host` に既定の実装つきのメソッドを足した。名前は **`compile_eval`**、答えは **`Result<Vec<u8>, EvalError>`**
（新しい型 `EvalError { line: u32, message: String }`）。名前の理由: 同じ仕事（`eval` の文字列を行と周りの変数名
つきで翻訳する）を compiler crate では既に `sabiruby_compiler::compile_eval` と呼んでいて、`Host` の `Compiler`
はそれを呼ぶだけなので、同じ名前にすると対応が読める。エラーの型は、引数の型が `EvalOptions` なので対になる
`EvalError` にした（compiler crate の `CompileError` は診断の一覧を持つ別物で、同じ名前は避けた）。

既定の実装は `compile` を呼び、行は文字列の始まりの行（`opts.line`、今までの SabiRuby の答え）にする。`compile`
だけを実装した Host（rubevy の `EmbeddedHost`・`FileHost`）は今のまま動く: rubevy をスクラッチパッドに写し、
`[patch.crates-io]` で作業中の sabiruby と sabiruby-compiler を指して `cargo check --all-features --lib` が通り、
`eval` の SyntaxError を見る `tests/embedded_host.rs` も 7 つ通った（rubevy の repo には何も書いていない）。
`Compiler` は `compile_eval` を実装して最初の誤りの診断の行を返し、`compile` はその文言だけを返す。VM の `eval`
と `top_load` は `compile_eval` を呼んで "file {name} line {line}: {message}" にする。rc2 と 6 通り
（3 行目の構文の誤り、行 10 から始めた `eval`、生成の誤り、`instance_eval`、`class_eval`、`binding.eval`）を
比べて全部同じ。

### `Integer#fdiv(0)` と `Kernel#Rational`（方針「mruby の限界は仕様」）

- `int_fdiv`（src/numeric.c:312）は `mrb_as_float` の後、割る数が 0 なら `mrb_int_zerodiv`。受け手の幅によらず、
  0.0・-0.0・`Rational(0, 1)`・`Complex(0, 0)` も（どれも 0.0 になる）。`Float#fdiv`（`flo_div`）と Rational の
  `fdiv`（`num_fdiv`）は割る。`Integer#fdiv` に 1 行足した。
- `rational_new_b(mrb_as_bint(a), b)`: 幅の広い Integer の隣の NaN・Infinity は `mrb_as_int` の RangeError
  "float NaN out of range"（a が先、b が後）。有限の Float は切り捨てられて黙って違う値（`Rational(2**70, 1.5)` が
  rc2 で `(1180591620717411303424/1)`）なので、今までどおり Float の腕のまま。

### `exc-copy` と `stack-error-gc`

方針（黙って中身を失う値、GC の状態で変わる答えは C）で、README の「?」の表から C の表に移し、計画書の Y5 の
一覧に 2 行足した。コードは変えていない。計画書の Y5 の頭書きは「次の 7 つ」のままで、一覧は前から 8 つあり、
今 10 になった（数は本体に任せた）。

### 確かめ

- `cargo test --release --workspace`・`cargo test --workspace`（debug、別の target）: どちらも 276 通る。
- custom 68 / 66（bytes）/ 61（no-regexp）。`rc2_sweep_y4.rb` に `eval` の行、`fdiv`、`Rational` の節を足した
  （期待値は rc2 の出力）。
- `tools/mrbtest.sh`（3 ビルド）: utf8 2346、bytes 2269、no-regexp 1981（baseline と同じ。md は日付だけなので
  戻した）。`check_no_std.sh` OK、`unsafe` の追加なし。
- `compare.py diff`（`feaa871` の表に対して）: `auto:` の 3 つ（`fdiv` の 8、`Rational` の 2 と 6）が消え、新しい
  原因なし。X2 は `rc2_sweep_y4.rb` の長さだけ。`overview_diff.rb` の `eval` の行（29 行目）は rc2 と同じに
  なった（表の上は 1 行目の `require` の差のまま）。
- edge_compare: 30/17/1、858/33/34/2、883/43 で変わらず。生成物は戻した。

### 性能（命令ループの外に出した後）

A = `11e595c`（md5 `56c61da6…`）、B = この節のコミットの木（`a14eda18…`、`exec_frames` 0xcbf1）、`--core 2`、
5 巡、同時に 1 つだけ。回す前に `docker ps` が空、`vmstat` の idle 100%、A の合計 45.6 s で静かな回。

| 走行 | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---:|---:|---:|---:|---:|---:|
| A/B | +0.6% | +0.5% | +1.5% | +0.5% | -0.5% | **+0.7%** |
| A/A | +0.2% | -0.3% | -0.4% | -0.1% | +0.6% | **+0.0%** |

| 1 本ずつ | `bm_so_mandelbrot` | `loop_if_branch` | `loop_times` | `loop_while_add` |
|---|---:|---:|---:|---:|
| A/B（5 巡） | +0.9% | +2.1% | +1.4% | +1.9% |
| A/B（9 巡、最初の確かめ） | -0.0% | +1.6% | +1.7% | +1.6% |
| A/A（この回、5 巡） | -0.7% | +0.2% | +0.0% | +0.5% |
| A/A（Y4 の 9 巡） | -2.1% | -0.4% | -1.2% | +0.6% |
| A/A（Y4 の 5 巡） | -1.0% | -1.4% | -0.2% | +0.8% |

`bm_so_mandelbrot` は +7.6% から A/A の幅（-2.1〜-0.7%）の近くに戻った。`loop_*` は +2.9〜+7.9% から +1.4〜+2.1%
に下がったが、3 つとも 2 回の測りでそろって正の側にあり、A/A の 3 回の幅（-1.4〜+0.8%）を少し超える。命令ループの
分類も +1.5% で、A/A（-0.4%、前回 +1.1%）を少し超える。Y4 の切り分けで 2 つの変更を戻した `v1` は
`loop_if_branch` -0.2% だったので、残りも `exec_frames` の配置によるものと見ている（確かめていない）。指示どおり
ここでは直さずに止めた。表はスクラッチパッドの `y4/c-ab.log`、`y4/c-aa.log`。

## Y5. C の記録（コードは変えない）

計画書の Y5 の 10 個を、README の "Known deviations" と `tests/custom/` に記録した。コードは変えていない。
基準は main `102d988`。rc2 の答えは `tools/custom.sh` が作る `.rc.out`（`kishima/mruby:4.1.0-rc2`）で、
同じ 8 本を `kishima/mruby:4.1.0-rc2-utf8` でも回し、出力が 1 字も違わないことを確かめた（文字列の読み方に
よらない題材なので、既存の custom テストと同じく `# utf8-only:` は付けていない）。CRuby は 3.2.6。

### 進め方

最初に、10 個それぞれについて SabiRuby・rc2・CRuby の答えを小さなスクリプト（スクラッチパッドの
`y5/probe.rb`〜`p9.rb`）で取り、計画書と README の C の表が書く答えと SabiRuby の今の答えが合っているかを
見た。10 個とも計画書のとおりで、「C のつもりが実は直っていない」ものは無かった。そのうえで、rc2 のソース
（`/home/kishima/book/ref/mruby`、HEAD は `4.1.0-rc2` の `c17ffcc24`、作業ツリーに変更なし）で、答えを
作っている関数と行を読んでから頭書きと README を書いた。

### 原因ごと

- **`argc-splat`**: rc2 は `chomp`・`chomp!`・`to_i`（`src/string.c:4473, 4474, 4495`）と `alias_method`
  （`src/class.c:4679`）を MRB_ARGS_ANY で宣言し、数えるのは `mrb_get_args`。速い道 `get_args_fast` は数えて
  から読む（`src/class.c:1402`）が、`argc >= 15 || ci->kw` で降りる（`:1387`）。splat で渡すと引数は配列
  1 つに詰められ `ci->n == 15` になるので遅い道 `get_args_v` に行き、そこは 1 つずつ変換しながら読んで、
  数を見るのは最後（`:1913`）。だから `"abc".chomp(nil, nil)` は ArgumentError、`"abc".chomp(*[nil, nil])`
  は TypeError になる。表の説明（「splat のときだけ数える前に変換する」）の仕組みはこれだった。新しく
  `argc_splat.rb`。
- **`frozen-set`**: mruby-set の変更するメソッドは `mrb_check_frozen_value` を呼ばない（呼ぶのは `reset` と
  `flatten!` だけ、`set.c:1185, 1352`）。`frozen_set.rb`。CRuby の文言は中の Hash を名指しするが、
  FrozenError なのは同じ。
- **`set-uninit`**: 表は「空の `Set.new` が `|` を断り、`add` の後は通る」と書いていた。読むと、根は
  `dup` だった。`union` は Array に対して `dup.merge(enum)`（`mrblib/set.rb:117`）、`set_init_copy`
  （`set.c:233`）は `kh_replace` で写し、`kh_replace` は空の元に対して `data = NULL` にする
  （`include/mruby/khash.h:393-399`）。表の無い Set に `merge` が `add` すると `set_ensure_initialized`
  （`set.c:158`）が "uninitialized Set" を出す。だから `add` して `delete` で空に戻した Set も断るし、
  `Set.new.dup << 1` も断る。テスト `set_uninit.rb` にはこの 2 つも入れた。
- **`int-div-float`**: `int_idiv`（`src/numeric.c:299`）が `mrb_as_int` で割る数を機械の整数にする。
  `ceildiv` は mruby-numeric-ext の `-div(-other)`（`numeric_ext.rb:109`）なので同じ。NaN などで RangeError
  になる半分は Y2 で rc2 に合わせた（`rc2_sweep_y2.rb`）ので、頭書きで両方を並べた。`int_div_float.rb`。
- **`eval-scope-custom-only`**: テスト（`eval_class_eval_scope.rb`、`eval_outer_scope.rb`）は前からあるので、
  足したのは README の 2 行だけ。テストの頭書きは変えていない。`eval_outer_scope.mrb` は
  `tests/inspect.rs` も読んでいて、頭書きの行を増やすと `.mrb` の行番号が動くため（変えても通るかは
  確かめていない。変える理由が無いので触らなかった）。README は「a clear defect」の節（master で直った
  もの・mruby の中で食い違うもの）に置いた。
- **`kw-dropped`**: 計画書は「rc2 はキーワードを受けないネイティブでは Hash を黙って捨てる」と書くが、
  確かめると捨てるのは一部だった。`mrb_get_args` の遅い道は空でないキーワードの Hash を位置引数に 1 つ
  足す（`get_args_v`、`src/class.c:1554-1577`）ので、`"a".center(k: 1)` や `[1, 2].rotate(k: 1)` は rc2
  でも TypeError。捨てるのは、`mrb_get_argc`／`mrb_get_argv`（`src/class.c:1249, 1277`、`ci->n` しか
  数えない）を先に読むメソッドで、`push`（`mrb_ary_push_m`、`src/array.c:852`）、`first`（`:1519`）、
  `Integer#to_s`（`src/numeric.c:2125`）など。`[1][k: 1]` は `mrb_get_argc` で 0 個と見てから
  `mrb_get_args("oi")` が Hash を 1 個と数え、"given 1, expected 2" になる。README の push の項の古い理由
  （「mruby は他では位置引数として渡す」）は `mrb_get_args` の側だけを見ていて、捨てる側も同じ rc2 に
  ある。README は「rc2 には規則が 1 つではない、読み方しだいで黙って消える」と書き直し、push の項と 1 つに
  まとめた。テストは `push_keywords.rb` に足した（README・CHANGELOG・過去の worklog がこの名前を指して
  いるので名前は変えなかった）。master `891d1d9d` の像でも同じ答えであることを確かめて頭書きに書いた。
- **`bigint-float`**: 4 つの道がある。`int_quo`（`src/numeric.c:329`）は割る数が機械の Integer のときだけ
  Rational で、それ以外は Float（`0.quo(2**70)` が 0.0）。受け手は `mrb_integer(x)`（`:340`）で型を見ずに
  読むので、`(2**70).quo(3)` は `(0/1)`、`(2**70 + 5).quo(7)` は `(5/7)`（下の 64 bit に当たる値を読んで
  いるように見えるが、そう言い切れるほどは確かめていないので README には観察した答えだけ書いた）。
  `int_remainder`（mruby-numeric-ext `numeric_ext.c:55`）は機械の Integer でない割る数を Float の道へ
  （`:82`）。`Rational(wide, f)` は `rational_new_b`（mruby-rational `rational.c:253`）が分母を
  `mrb_as_bint` で狭めるので 1.5 が 1 になる。SabiRuby の `Rational(2**70, 1.5)` は
  `(787061080478274158592/1)` で、これは rc2 が `Rational(4, 1.5)` を答えるやり方（Float で割ってから
  Rational にする）と同じ。CRuby の `(2361183241434822606848/3)` とも違うことを README と頭書きに書いた。
  `bigint_float.rb`。
- **`fill-empty-range`**: テストは Y2 の `fill_empty_range.rb`（期待値は CRuby）がある。頭書きの「the
  record is Y5」の行の後に README を指す一言を足し（行は増えないので `.mrb` は変わらない）、README に
  「a clear defect」の節の 1 行（未初期化のメモリを読む＝未定義動作）を足した。
- **`exc-copy`**: `init_copy`（`src/class.c:4349`、MRB_TT_EXCEPTION の腕は `:4366`）は ivar しか写さず、
  `struct RException` の `mesg` と `backtrace` は写らない。`exc_exception`（`src/error.c:87`）は clone して
  文言だけ入れ直すので backtrace を失う。`exc_copy.rb`。
- **`stack-error-gc`**: `clear_error_object`（`src/gc.c:1484`、`final_marking_phase` の `:1528` から）が
  作り置きの `stack_err` の文言と backtrace を消す。テスト `stack_error_gc.rb` は GC の前に 1 回、
  `GC.start` の後に 1 回 `def f; f; end; f` を呼ぶ。rc2 は前が "stack level too deep"、後が
  "SystemStackError"。プログラムが小さいので最初の呼び出しまでに GC は走らず、rc2 の答えは毎回同じ。

### ファイルの分け方

既存の custom テストは「記録した差 1 つにファイル 1 つ」（`argnum_given_kw.rb`、`integer_infinite.rb`、
`push_keywords.rb` など）なので、それに合わせて新しい 7 本を足した: `argc_splat`、`frozen_set`、
`set_uninit`、`int_div_float`、`bigint_float`、`exc_copy`、`stack_error_gc`。頭書きは既存と同じ形
（rc2 のどこか、なぜこちらを採ったか、`# expected-from: SabiRuby (CRuby 3.2 says the same)`、
`README, "Known deviations".`）。`.expected` は SabiRuby の出力をそのまま写し、CRuby 3.2.6 の出力と
1 行ずつ見比べた（違いは CRuby の文言と inspect の書き方、それと `bigint_float` の 2 つの
`Rational(_, 1.5)` で、頭書きに書いた）。`kw-dropped` は既存の `push_keywords.rb` に、
`eval-scope-custom-only` と `fill-empty-range` は既存のテストに任せた。

README は、方針の 2026-10-18 の節の見出しを「mruby の中で食い違う、データを失う、または黙って違う値か
GC の状態で変わる値を答える」に広げ、そこに 8 項（kw、argc-splat、frozen-set、set-uninit、int-div-float、
bigint-float、exc-copy、stack-error-gc）、「a clear defect」の節に 3 項（eval の 2 つと fill）を置いた。
CHANGELOG の Unreleased に 1 段落。

### 確かめ

- custom: 75 / 73（bytes、`--no-default-features --features std,regexp`）/ 68（no-regexp、
  `--features std,utf8`）。前の 68 / 66 / 61 から 7 本ずつ増えた。
- `cargo test --release --workspace`・`cargo test --workspace`（debug、別の target）: どちらも 276 通る。
- `tools/mrbtest.sh`（3 ビルド）: utf8 2346、bytes 2269、no-regexp 1981（baseline と同じ。md の差は
  日付だけなので戻した）。`check_no_std.sh` OK。コードを変えていないので `unsafe` もベンチも無し。

### 迷った点

- **`kw-dropped` の rc2 の書き方**: 計画書の箇条（「キーワードを受けないネイティブでは捨てる」）より実物は
  狭い。SabiRuby の答えは計画書のとおりなので止まる場面ではないと判断し、README は確かめた事実
  （`mrb_get_args` は位置引数にする、`mrb_get_argc` を先に読むものは捨てる）で書いた。
- **「1 つずつテストを足す」と既存のテスト**: eval の 2 つと fill は既にテストが記録になっているので、
  重ねて足さなかった。
- **`stack-error-gc` の同一性**: rc2 の SystemStackError は毎回同じオブジェクト（`a.equal?(b)` が true）、
  SabiRuby は毎回作る（false）。同じ根だが計画書の項目は文言なので、テストにも README にも入れず、
  下の気づいた点に書いた。

## 気づいた点（Y5）

- **`(2**70).remainder(1.5)` は rc2 も SabiRuby も 0.0、CRuby は 1.0**（2^70 を 3 で割った余りは 1 なので、
  1.5 で割った余りは 1.0 が正しい）。rc2 と同じなので洗い出しには出ない。SabiRuby が rc2 と同じ式
  （Float の `fmod` など）で答えているのかは見ていない。属する先: VM（mruby の限界か不具合かの判断は著者）。
- **SystemStackError の同一性**: rc2 は作り置きの 1 つ（`mrb->stack_err`）を毎回投げるので、2 回の
  `rescue` で受けた例外が `equal?`。SabiRuby は別のオブジェクト。CRuby も別。記録するかは本体の判断。
- **rescue 修飾子の中の `$!`**: `p((raise "a" rescue $!))` は rc2 も SabiRuby も nil、CRuby は例外。
  両方そろっているので差ではない。本（`$!` を扱う所があれば）の素材になりうる。

## Y6. 全体の確かめ

基準: main `842d8ed`（Y5 の後）。VM・CLI・ほかの crate のコードは変えていない。変えたのは `tools/rc2_sweep/`
（`compare.py`・`causes.py`・`programs.py`）と docs だけ。

### 回し直す

Y4 までと同じ手順。`cargo build --release -p sabiruby-cli`（md5 `a5025935…`）、`target/rc2-sweep` を
`target/rc2-sweep-y5` に移して `lists`・`cases`（4 つの一覧と `chunks.json` は前の走行とバイト単位で同じ）、
`run --timeout 5 --only '^(?!Range:endless$)'`（54 s、restarts 0）、`(1..)` の raw は前の走行から写して
`compare.py`、`programs.py run --timeout 60`（69 s、上限に当たったのは前と同じ `test_NOP_empty_loop.rb` の
4 つだけ）と `compare`。

**HEAD（Y4 の続きの走行の表）との差**: X1 は `no change`（Y5 はコードを変えていないので、そうなるはず）。
X2 は Y5 で足した custom の 7 本が増え（どれも `stdout` の差。記録した C なので差があって正しい）、
`push_keywords.rb` の出力の長さが変わった（Y5 で足した 6 行）。`vm_task_sleep_switching.rb` の長さの
1 バイトは Y0 の揺れ。

**最初の洗い出し（`9375b09` の表）との差**（`compare.py diff --base 9375b09`、規則を足す前）:

```
X1 calls compared / without an answer: 977521 / 4345 (same)
X1 causes: 92 -> 54; 1 new, 39 gone, 10 changed
  gone   A: argc-narrow, argc-wide, binding-inspect, bytesplice, conv-order, crash, enum-args-kw,
            fiber-inspect, join-empty, lenient-arg, math-nan-domain, merge-partial, method-missing-name,
            negative-zero, proc-init-copy, rational-minus, regexp-match-nil, set-reinit, split-empty,
            sum-block, time-year, visibility-args
         B: class-new-super, conv-float-msg, conv-int-msg, conv-msg-other, data-init, module-required,
            not-a-symbol, struct-const-name, type-nan-name, unknown-keyword
         ?: int-div-nonfinite
         auto 4 (ZeroDivisionError と float out of range の 3 つは Y4 の続き `80fc665`、String#split は Y2)、
         time-now（揺れ）、known: a NaN has no identity（3、Y2 で値が揃った）
  changed  A: reinitialize 3970->748, frozen 1136->32, bigint-to-float 152->12
           B: msg-other 26->2
           C: argc-splat 12042->12834（Y2 で chomp! の splat の呼び出しが frozen・argc-wide から移った）
           ?: kw-dropped 706->691
           D: internal-helper 15178->14997, float-ulp 20->22, known 2 行の増減
  new      known: define_singleton_method(name, a_proc)（Y2 で規則を広げた）
X2 x2-programs.tsv: 502 -> 517 rows; 15 added, 0 removed, 22 changed
X2 x2-asserts.tsv: 250 -> 250 rows; 0 added, 0 removed, 0 changed
```

段ごとのコミットにある表（`46e79f6`、`6b0c4a1`、`144d6a0`、`feaa871`、`80fc665`、`842d8ed`）を
`git show` で読み、原因ごとの呼び出しの数がどの段で 0 になったかを並べた。README の A・B・C・? の表に足した
「done」の列はこの並びから書いた（Y1 は `bytesplice` の 4,434 を 2,175 にした（2 つの数の形）、Y2 が残り）。

### 残った A と B を 1 行ずつ

この時点で A の名前が付いた原因は 3 つ、B は 1 つ残っていた。

- **`reinitialize` 748**: 全部 `(1..)` の受け手で、raw は前の調査のものを写し続けていた（Y2 の
  気づいた点）。走らせなければ確かめられないので、下の「`(1..)` を走らせる」で走らせた。
- **`frozen` 32**: Y2 は「`Set#replace` の 30 は C の `frozen-set`、`[]=` の 2 は記録済み」と書いていた。
  1 行ずつ見ると、
  - `Set[1].freeze.replace(nil)` など 30: rc2 は ArgumentError "value must be enumerable"、SabiRuby は
    FrozenError。CRuby 3.2.6 も ArgumentError。数えられる値を渡せば
    rc2 は凍結した Set を変える（これは `frozen-set`）が、数えられない値では rc2 と CRuby が同じ答えで、
    SabiRuby だけが先に凍結を見ている。見る順の差なので **A**。`frozen-set` には入らない。
    新しい原因 `set-replace-order` として分けた（直していない）。
  - `[3, 1, 2].freeze[1, 2**70] = ""` など 2: rc2 は FrozenError、SabiRuby は RangeError（長さを先に
    変換する）。凍結していない配列で同じ呼び出しをすると rc2 は `[3, "", 2]`（長さを見ずに添字を長さに
    使う）。README "Known deviations" の `a[i, len] = v` の項の根（`aget_index` の fallback が長さを見ない）
    そのものなので、Y2 の「記録済み」は正しい。ただし `known()` の規則は長さが数でない値（nil、文字列…）
    だけを見ていて、`2**70`（機械の整数でない Integer）を外していた。規則を広げた（下）。凍結していない
    配列の同じ形の 6 呼び出し（`auto: e rc2 ok / sabiruby raise RangeError`）も同じ規則に入る。
- **`bigint-to-float` 12**: `(2**70) & 1.5`、`|`、`^`、`allbits?`、`anybits?`、`nobits?`。rc2 は 1.5 を
  切り捨てて答え（`(2**70) & 1.5` が 0）、小さい受け手の `7 & 1.5` は TypeError。SabiRuby は両方 TypeError、
  CRuby も TypeError。Y2 の迷った点で「黙って違う値なので SabiRuby のまま、C の記録に足すかは本体の判断」と
  したもの。Y5 の 10 個には入っておらず、README "Known deviations" に項が無い。方針では C（黙って違う値で、
  受け手の大きさで答えが変わる）だが、**記録されていない C** として新しい原因 `bigint-bitop-float` に分けた。
  記録（README の項と custom のテスト）は本体の判断に残す。
- **`msg-other` 2**: `(1..3).lazy.sort` の SystemStackError の文言。Y5 で `stack-error-gc` として記録した
  もの。`known()` に規則を足して k にした。

### C を k に: `known()` と `RECORDED`

`compare.py` の `known()` は呼び出しの形（受け手・メソッド・引数）で記録済みかを決める。C の 8 原因
（X1 に出るもの: `argc-splat`、`frozen-set`、`set-uninit`、`int-div-float`、`kw-dropped`、`bigint-float`、
`fill-empty-range`、`stack-error-gc`）の範囲は `causes.py` の規則がもう決めているので、同じ条件を
`known()` に書き写すと 2 か所が食い違いうる。そこで `known()` で決まらなかった呼び出しは先に原因を出し、
その原因が記録済みの C の表 `RECORDED`（原因 → README の項の名前）にあれば k にする形にした。原因の範囲は
記録の前後で変わらない。`kw-dropped` と、前からある `push` の k は README で 1 つの項になった（Y5）ので、
同じ名前（`KW_RECORD`）にまとめた。`stack-error-gc` は `msg-other` の一部なので原因の表ではなく、
`known()` に 1 行（SystemStackError で rc2 の文言が "SystemStackError"）を足した。

`exc-copy` は X1 に出ない。`dup`・`clone` は rc2 では Kernel のメソッドで、洗い出しは Kernel のメソッドを
`Object.new` にしか呼ばない（`cases.py` の `OBJECT_OWNERS`）。`Exception#exception(msg)` は呼んでいるが、
比べるのは値の inspect と受け手なので backtrace の違いは見えない。`eval-scope-custom-only` は X2 だけ。
どちらも README に記録済み（Y5）で、X2 では custom のテストの行（D）になる。

### 自動の群を 1 行ずつ

最初の洗い出しで「分類しない」とした自動の群（31 群、124 呼び出し）のうち、27 群 107 呼び出しが残って
いた（4 群は Y2・Y4 で直った）。どれも最初の表にあったもので、**新しく出た原因は無い**。計画は「残る差は
行ごとに見立てを付ける」なので、1 群ずつ rc2・SabiRuby・CRuby 3.2.6 で 1 行の再現を取り
（スクラッチパッドの `y6/p1.rb`〜`p3.rb`）、rc2 のソース（`/home/kishima/book/ref/mruby`、HEAD
`c17ffcc24` のまま）で答えを作る所を読んで見立てを付けた。記録済みの根に入るものは `known()` を広げ、
それ以外は `causes.py` に原因を足した（最後の段に置いたので、前からの原因の行は 1 つも動かない）。

記録済みの根に入れたもの（k）:

- `[]=` の長さが `2**70`（6）と凍結した配列の 2: 上の `a[i, len] = v`。
- `Integer#to_s(2**70)`・`inspect(2**70)`（8）: README の項が「幅の広い Integer は "invalid radix 0"」と
  書いているのに、規則が `2**70` を外していた。
- `Complex#infinite?`（4）: rc2 は Integer と同じ `num_infinite_p`（false）を通る。README の項は Integer・
  Rational を名指しし Complex を書いていないので、規則のコメントにそう書いた。洗い出しの Complex は有限の
  値だけ。`Complex(Float::INFINITY, 2).infinite?` は rc2 false、SabiRuby nil、CRuby 1 で、SabiRuby も
  CRuby と違う（気づいた点）。
- `sort`・`sort!` にブロック `{ |*x| x[0] }`（11）: 洗い出しのブロックは順序になっていない（要素そのものを
  返す）ので、どの組をどの順で渡すかで並びが変わる。README の sort の項（渡す組が違う）の根。規則は
  「並べた結果が同じ要素の別の並び、または同じ例外」に限った。
- `"あいう".delete_prefix("あ")`（2）: `docs/design/utf8.md` の、バイトの位置を文字の位置として切る
  5 つのメソッド。
- `(1..3).lazy.sort`（2）: `stack-error-gc`（上）。

原因を足したもの（見立てと、rc2 の答えを作る所）:

| 原因 | 呼び出し | 見立て | 理由 |
|---|---:|---|---|
| `set-replace-order` | 30 | A | 上。rc2 と CRuby が同じ（ArgumentError）、SabiRuby だけ凍結を先に見る |
| `math-log-order` | 16 | A | `Math.log(-1, nil)`: rc2 は底を先に変換して TypeError、SabiRuby（と CRuby）は第 1 引数の定義域を先に見て Math::DomainError。変換の順の差で、rc2 の答えに不具合は無い |
| `alias-same-name` | 10 | A | `Class.new.alias_method(:x, :x)`: rc2 の `mrb_alias_method`（`src/class.c:3674`）は 2 つの名前が同じならすぐ戻るので、無いメソッドでも通る。SabiRuby と CRuby は NameError。`alias x x` も同じ |
| `match-pos-past-end` | 6 | A | `"".match("", 1)`: 位置が文字列の外で rc2 は nil／false。SabiRuby はそこで照合して空のパターンが合う。CRuby は MatchData（`match?` は false）で CRuby 自身が揃っていない。rc2 の答えは筋が通っている |
| `undef-method-answer` | 4 | A | rc2 の `mrb_mod_undef`（`src/class.c:3988`）は常に nil、SabiRuby（と CRuby）はモジュール。洗い出しでは引数 0 個だけが差に出る（引数のある呼び出しはどれも両方 NameError） |
| `shift-negative` | 2 | A | `[3, 1, 2].shift(-1)`: rc2 ArgumentError "negative array shift"（CRuby も ArgumentError）、SabiRuby は配列全体を返す。SabiRuby の答えは誰の答えでもない |
| `fiber-root-status` | 2 | A | `Fiber.current` をトップレベルで: 根のファイバー（`mrb->root_c`）の状態は初期値の created のまま（mruby-fiber の `fiber.c` が状態を書くのは切り替えのとき、`src/vm.c:929` はファイバーが終わって戻るとき）なので、rc2 は "(created)"。SabiRuby と CRuby は "(resumed)"。inspect の書き方で、不具合と言い切れないので方針どおり A |
| `time-init-failed` | 22 | C（未記録） | 生きた Time に `initialize` が失敗すると、rc2 は先に中身を捨てている（`mrb_data_init(self, NULL)`、mruby-time）ので受け手の inspect が以後 ArgumentError。SabiRuby は元のまま。失敗した呼び出しが受け手の値を失わせる（方針の「データを失う」） |
| `bigint-bitop-float` | 12 | C（未記録） | 上 |
| `bigint-pow-mod` | 8 | C（未記録） | rc2 の bigint の道の `pow(n, m)`: Float の法を切り捨て（`(2**70).pow(2, 1.5)`、`7.pow(2**70, 1.5)` が 0、`(2**70).pow(-1, 1.5)` は "n must be positive"）、小さい道の `7.pow(2, 1.5)` は TypeError。`(2**70).pow(0, 1)` を 1 と答える（`7.pow(0, 1)` は 0、CRuby も 0）。黙って違う値で、受け手の大きさで答えが変わる。Y2 の迷った点の `(-3).pow(2**70, 1.5)` と同じもの |
| `const-get-empty` | 4 | C（未記録） | `Class.new.const_get("")`: rc2 の `mrb_mod_const_get`（`src/class.c:4039`）は文字列の道の繰り返しが 1 度も回らず、受け手そのものを返す。`const_get(:"")` は NameError。同じ問いに Symbol と String で答えが違い、黙って違う値。SabiRuby と CRuby は NameError |
| `native-params` | 2 | ? | 下 |

C（未記録）の 4 つは、方針では C だが Y5 の 10 個に入っておらず、README "Known deviations" に項が無い。
新しい直しのラウンドにしない（計画）ので、表に足して報告に回した。SabiRuby の答えは CRuby と同じで、
コードを変える話ではない（記録するかどうかだけ）。A の 7 つは SabiRuby のコードを変えないと合わない。

### ネイティブの `parameters`（Y3 の報告）

Y3 の報告にあった「rc2 の `[[:opt]]` と SabiRuby の `[[:req]]`」は、洗い出しでは `auto: v Method#parameters`
の 2 呼び出しだった。受け手 `Method` が `1.method(:+)` なので、出るのはこの 1 つの形だけ。両方で
いくつかのネイティブを見た（`y6/p3.rb`）:

| メソッド | arity（両方） | rc2 | SabiRuby |
|---|---:|---|---|
| `Integer#+`、`-`、`*`、`<=>`、`Float#+` | 1 | `[[:opt]]` | `[[:req]]` |
| `Array#first`、`Integer#to_s` | -1 | `[[:opt]]` | `[[:rest]]` |
| `String#center` | -2 | `[[:opt], [:opt]]` | `[[:req], [:rest]]` |
| `String#sub` | -1 | `[[:rest], [:block]]` | `[[:rest]]` |
| `Array#push` | -1 | `[[:rest]]` | `[[:rest]]` |
| `Hash#fetch`（両方 Ruby） | -2 | 同じ | 同じ |

根は 2 つある。

1. rc2 の `mrb_proc_parameters`（mruby-proc-ext）は C の関数を名前の無いブロックのように扱い、必須の引数も
   `:opt` と書く（arity は 1 なのに `[[:opt]]`）。Ruby で書いたメソッドは `[[:req, :x]]`。同じ宣言のメソッド
   が C か Ruby かで答えが変わるので、方針の「mruby の中で食い違う」に当たるとも読めるし、rc2 の書き方として
   写すべきとも読める。写すと SabiRuby のネイティブの必須の引数が全部 `:opt` になり、利用者に見える。**? とした**（著者の判断）。
2. SabiRuby の `Aspec` が rc2 の宣言と違うもの（`first`、`to_s` の opt を rest、`center` の opt を rest、
   `sub` の block）。arity は同じなので洗い出しの呼び出しには出ない（引数の数の差は Y2 の `argc-*` で
   本体が見ているので、宣言だけが違う）。洗い出しの受け手は `1.method(:+)` しかないので、表の数には
   入っていない。A の候補として気づいた点に書いた。

### `(1..)` を走らせる

`reinitialize` の 748 を確かめるには `(1..)` を走らせるしかない。全部（16,388 呼び出し）を回すと、答えの
無い 4,345 呼び出しが 1 つ 5 秒ずつ上限に当たり、数時間かかる。そこで、写してきた raw で両側に答えのあった
12,043 呼び出しだけを、同じ上限（5 秒）で走らせ直した。道具は使い捨てのスクリプト（スクラッチパッドの
`y6/endless.py`）で、`sweep.py` の `Chunk` をそのまま使い、塊のソースに「答えの無かった番号は飛ばす」
4 行を足しただけ（番号は元の塊と同じなので、出力の raw はそのまま `target/rc2-sweep/raw/` に置ける）。
メモリの上限は `sweep.py run` の既定と同じ式（`MemAvailable` の半分を同時に走るプロセスの数で割る、
ここでは両側 1 つずつで 2）。作業ディレクトリは `target/rc2-sweep-endless/`。

所要 968 s（両側を並べて。遅い方の SabiRuby 側）。restarts は rc2 154、SabiRuby 182 で、写してきた raw の
timeout の数と同じ（同じ番号で上限に当たる）。答えが変わった行は rc2 134、SabiRuby 882。rc2 の 134 は全部
塊のファイルのパスだけの違い（`Enumerator::Chain` の inspect に Proc の定義場所が出る。作業ディレクトリを
分けたため）で、パスを置き換えると 0 行。SabiRuby の 882 は同じ 134 と、`Range#initialize` の 748。
`target/rc2-sweep/raw/` に置くとき、パス `target/rc2-sweep-endless/chunks/` を `target/rc2-sweep/chunks/` に
書き換えた（次に写して使うときに前の走行と同じに見えるように）。`compare.py` の結果で変わったのは
`reinitialize` の 748 が 0 になったことだけ（`same` が 944,236 から 944,984 へ 748 増えた）。
Y2 で手で 1 つ確かめたとおり、`(1..).__send__(:initialize, …)` は rc2 と同じ答えになっている。

### 道具: X2 の「直ったのに見えない」行

`x2-programs.tsv` は stdout の差を「最初に違う行」（`first_stdout_diff`）でしか持たない。そのため、
後ろの行が直っても前の行が別の原因で違ったままなら、表は何も変わらない。Y3 の `method-owner`
（`corelib_kernel.rb` の 3 行目、前の 2 行目は `one-side-methods`）と、Y2・Y4 の `overview_diff.rb`
（21 行目の `String#+`、29 行目の eval の行。前の 1 行目は `require`）がそうだった。

列を足すことにした: `stdout_diff_lines`、違う行の番号を全部、続く番号は範囲にまとめて（`2-3,6-7`）。
注記だけにしなかったのは、道具が毎回表を書き直して `compare.py diff` が列ごとに比べる作りなので、列にすれば
次からは直った行が `diff` に `stdout_diff_lines: '2-3,6-7' -> '2,6-7'` の形で出るため。行は
`first_diff` と同じく位置で比べるので、1 行多く印字すると後ろが全部違って見える（関数の説明に書いた）。
stderr には足していない（stderr が違うのは gem の差（`$stdout` が無い、Task が無い）と記録済みの `eval_outer_scope.rb` だけで、どれもプログラムがそこで止まるので、後ろの行が直って見えなくなることが無い）。
`diff` は古い表にある列だけを比べるので、列の無い `9375b09` の表とも比べられる。

過去の走行の `results.json`（`target/rc2-sweep-*/x2/`）に同じ関数をかけると、どの段で直ったかが見える:

| プログラム（book と kit-samples の同じもの） | 調査 | Y1 | Y2 | Y3 | Y4 | Y4 の続き | Y6 |
|---|---|---|---|---|---|---|---|
| `corelib_kernel.rb` | 2-3,6-7 | 2-3,6-7 | 2-3,6-7 | **2**,6-7 | 2,6-7 | 2,6-7 | 2,6-7 |
| `overview_diff.rb` | 1,21,29 | 1,21,29 | **1,29** | 1,29 | 1,29 | **1** | 1 |
| `corelib_error_messages.rb` | 6 | 6 | 6 | 6 | 同じ | 同じ | 同じ |
| `exp_GETCONST_frozen.rb` | 5-7 | 同じ | 同じ | 同じ | 同じ | 同じ | 同じ |
| `exp_TDEF_target_class.rb` | 5-7,10-12 | 同じ | 同じ | 同じ | 同じ | 同じ | 同じ |

（「調査」は `target/rc2-sweep-prev`、ほかは各段の終わりの走行。Y0 の走行は調査と同じなので省いた。）
`x2-causes.tsv`（手で書いた表）にも status の列を足し、X2 の原因ごとにどの段のどのコミットで直したか／
記録したかを書いた。

### 確かめ

- mrbtest（3 ビルド、`tools/mrbtest.sh`）: utf8 2508 / ok 2346 / ko 51 / crash 100 / skip 11、bytes 2453 /
  2269 / 38 / 90 / 56、no-regexp 2012 / 1981 / 9 / 17 / 5。baseline と同じ。生成した md の差は日付だけなので
  戻した。
- `tools/custom.sh`: 75 本、agrees 49 / DIFFERS 26（DIFFERS は `.expected` を rc2 と違うと決めたもの）。
  `.mrb`・`.dump`・`.rc.out` に git の差なし。`cargo test --release --test custom`: 75 / 73（bytes、
  `--no-default-features --features std,regexp`）/ 68（no-regexp、`--no-default-features --features
  std,utf8`）、どれも 0 problems。
- `tools/edge_compare.sh`: ファイル 30 passed / 17 failed / 1 runtime_error、assert は pass 858 / fail 33 /
  error 34 / skip 2、`assert.rb` の参考の列は ok 883 / ko 43。Y4 と同じ（生成物の差はアドレス・時間・
  コミットだけ）なので戻した。
- VM のコードは変えていないので `cargo test` 全体・ベンチは回していない。

### 迷った点

- **Y2 が「C」「記録済み」とした残りの見直し**: `frozen` の 30 は Y2 の書き方（C の `frozen-set`）と
  違い、rc2 と CRuby が同じ答えの A だった。`bigint-to-float` の 12 は C だが記録が無い。どちらも直さず
  （コードを変える話になるので）、表で原因を分けて報告に回した。
- **記録済みの根に入れるか、新しい原因にするか**: README の項の文がその値を名指ししていなくても、項が書く
  根（rc2 のどの関数が何を見ないか）と同じものは k にした（`2**70` の長さ、`Complex#infinite?`、意味の無い
  ブロックの `sort`）。どれも規則のコメントに「README の文は名指ししていない」と書いた。README の文を広げるか
  は本体の判断。
- **`fiber-root-status` と `match-pos-past-end` を A にしたこと**: どちらも rc2 の答えが不具合と言い切れ
  ないので方針どおり A にした。`time-init-failed` は「データを失う」に当たるとして C にした。
- **`native-params` を ? にしたこと**: 上。
- **`(1..)` を全部は走らせなかったこと**: 答えの無い 4,345 呼び出しは前の調査から一度も答えが無く、今回も
  飛ばした。表の「答えの無い数」は 4,345 のまま。

### 最後の走行と HEAD・調査の表

規則を足し、`(1..)` を入れた後の `compare.py diff`（HEAD = Y4 の続きの走行の表）:

```
X1 calls compared / without an answer: 977521 / 4345 (same)
X1 causes: 54 -> 36; 21 new, 39 gone, 5 changed
  gone   A: reinitialize (748), frozen (32), bigint-to-float (12)
         B: msg-other (2)
         C: argc-splat, frozen-set, set-uninit, int-div-float      （k へ）
         ?: kw-dropped, fill-empty-range, bigint-float              （著者の判断で C、k へ）
         auto 27 群（全部）、known: [].push(1, k: 2) …（kw-dropped の項に名前をまとめた）
  new    A: set-replace-order 30, math-log-order 16, alias-same-name 10, match-pos-past-end 6,
            undef-method-answer 4, shift-negative 2, fiber-root-status 2
         C（未記録）: time-init-failed 22, bigint-bitop-float 12, bigint-pow-mod 8, const-get-empty 4
         ?: native-params 2
         known（C の記録）: argc-splat 12,834、frozen-set 1,251、kw-dropped と push 694、fill 87、
            set-uninit 30、bigint-float 24、int-div-float 12、stack-error-gc 2、utf8 の 2
  changed known: a[i, len] 288->304、Integer#to_s 144->152、sort 5->16、infinite? 12->16、
            Array の Float の添字 728->720（a[i, len] の規則が先に取る 8）
X2 x2-programs.tsv: columns only now: stdout_diff_lines (not compared)
X2 x2-programs.tsv: 510 -> 517 rows; 7 added, 0 removed, 2 changed   （Y5 の custom 7 本、push_keywords、揺れ）
X2 x2-asserts.tsv: 250 -> 250 rows; 0 added, 0 removed, 0 changed
```

調査（`9375b09`）とは `X1 causes: 92 -> 36; 22 new, 78 gone, 8 changed`。new は上の 12 原因（どれも調査の
自動の群か、`frozen`・`bigint-to-float` の残りから名前を付けたもの）と known の行。X2 は 502 -> 517 行、
15 足され（直しのテスト）、22 変わった（`differs` が same になった 3 本とその kit の写し、gc の数の揺れ）。

見立ての数は、調査が A 25（X2 で +3）・B 11（+1）・C 4（+1）・? 4（+1）・自動 31 群だったのが、A 7・B 0・
C 4（未記録）・? 1・自動 0。差のある呼び出しは 68,583 から 32,537 で、そのうち 16,268 が k、16,151 が D
（`internal-helper` 14,997 と `set-order` 900 が大きい）。README の冒頭に「After the fixes」の節を足し、
この表と残りの 1 行ずつ、洗い出しに出ない 2 行（下）を書いた。調査の要約と原因の表は残し、A・B・C・? の表に
「done」の列（段とコミット）を足した。`x2-causes.tsv` には status の列。

### 洗い出しに出ない 2 行（計画の箇条）

- **`(2**70).remainder(1.5)`**: rc2 も SabiRuby も 0.0、CRuby は 1.0（2**70 = 1.5 × q + 1）。両方とも
  `a - b * trunc(a / b)` を Float で計算する（rc2 `flo_remainder`、mruby-numeric-ext `numeric_ext.c:432`、
  SabiRuby `src/builtins/ext_numeric.rs:39` の同名の関数、`int_remainder` は幅の広い受け手で Float の道へ
  行く）ので、大きな `a` で余りが消える。`(2**70) % 1.5` は両方 1.0。rc2 と同じ答えなので洗い出しには
  出ない。黙って違う値なので方針では **C の候補**。直すならコードの変更と記録で、直していない。README に
  `float-remainder-precision` として 1 行。
- **SystemStackError の同一性**: rc2 は作り置きの 1 つ（`mrb->stack_err`）を毎回投げるので、2 回の
  `rescue` で受けたものが `equal?`。SabiRuby は毎回作る（false、CRuby も false）。実装の違いなので **D**。
  README に `stack-error-identity` として 1 行。文言の方は `stack-error-gc`（記録済み）。

## 気づいた点（Y6）

- **`Complex(Float::INFINITY, 2).infinite?`** は rc2 false、SabiRuby nil、CRuby 1。SabiRuby も CRuby と
  違う。洗い出しの Complex は有限の値だけなので表には出ない（有限の値は README の `infinite?` の項の根で k
  にした）。README の項は Complex を名指ししていない。属する先: VM（Complex#infinite? の答え）と README。
- **README "Known deviations" の文と `known()` の規則の範囲**: `a[i, len] = v` の項は「長さが数でない」と
  書くが、`2**70` の長さも同じ根で、規則はそれも取るようにした。同じく `Complex#infinite?`、順序でない
  ブロックの `sort`。項の文を広げるかは本体の判断。属する先: docs。
- **記録されていない C が 4 つ**（`time-init-failed`、`bigint-bitop-float`、`bigint-pow-mod`、
  `const-get-empty`）。SabiRuby の答えは CRuby と同じでコードの変更は要らないが、README の項と custom の
  テストが無い。属する先: 著者（記録するか）。
- **A が 7 つ残っている**（`set-replace-order`、`math-log-order`、`alias-same-name`、`match-pos-past-end`、
  `undef-method-answer`、`shift-negative`、`fiber-root-status`）。どれも小さい（S）。`shift-negative` は
  SabiRuby の答え（`[3, 1, 2].shift(-1)` が配列全体）が誰の答えでもない。属する先: VM（次の直しの段）。
- **ネイティブの `Aspec` が rc2 の宣言と違うもの**（`Array#first`、`Integer#to_s`、`String#center`、
  `String#sub` …）。arity が同じなので呼び出しでは見えず、`parameters` にだけ出る。洗い出しの受け手には
  `Method` が 1 つ（`1.method(:+)`）しかないので、数えられていない。全部を数えるなら、両方の VM で全ネイティブ
  の `parameters` を取って比べる道具が要る（`methods.rb` に足せば `lists` で取れる）。属する先: 道具／VM。
- **`compare.py diff` は列の増えた表を比べると全部の行を「変わった」と言っていた**（直した: 片方にしか無い列は
  名前を 1 度出して比べない）。属する先: 道具（直した）。
- **`(1..)` を部分的に走らせる手順は使い捨て**（スクラッチパッドの `y6/endless.py`）。答えのあった番号だけを
  走らせる形は `sweep.py run` の option にできる（例えば `--rerun-answered RECEIVER`）。今回は計画に無いので
  足していない。属する先: 道具。
- **洗い出しは Kernel のメソッドを `Object.new` にしか呼ばない**ので、例外の `dup`・`clone`（`exc-copy`）の
  ように、受け手ごとに中身の違う Kernel のメソッドの差は見えない。属する先: 道具（受け手ごとに Kernel の
  一部を呼ぶか）。

## Y6b. Y6 で見えた、線の内側の残り

基準: main `87401ec`（Y6 の後、計画に Y6b を足したコミット）。Y6 の表で残った A の 7 原因と `native-params`
を直し、記録の無い C の 4 原因を Y5 と同じ形で記録した。`Complex#infinite?` は CRuby の答えにして C として
記録した。rc2 の答えはどれも Docker の `kishima/mruby:4.1.0-rc2-utf8`（custom の `.rc.out` は
`kishima/mruby:4.1.0-rc2`、新しい 6 本は utf8 の像でも同じ出力であることを見た）で確かめ、ソースは
`/home/kishima/book/ref/mruby`（HEAD `c17ffcc24` のまま、作業ツリーに変更なし）を読んだ。CRuby は 3.2.6。
コミットは 1 つ（同じファイルに複数の原因が入るため）。

### 進め方

最初に、7 つの A の再現（Y6 の表の repro）を 1 つのスクリプト（スクラッチパッドの `y6b/p1.rb`）にまとめて
両方で走らせ、表の rc2 の答えがそのとおりかを見た。7 つとも表のとおりだった。そのうえで原因ごとに rc2 の
関数を読み、写してから同じスクリプトで差が消えたことを見た。`native-params` は数え漏れを防ぐため、直す前に
全メソッドの `parameters` と `arity` を両方で取って比べた（下）。

### 原因ごと（A）

- **`math-log-order`**: rc2 の `math_log`（mrbgems/mruby-math/src/math.c:477）は `"f|f"` で 2 つとも
  変換してから、`x < 0` と `base < 0` を見る。SabiRuby は `x` の定義域を見てから底を変換していた。変換を
  先に並べ替えた（`ext_math.rs`）。`Math.log(-1, 2)` はどちらでも Math::DomainError のまま。
- **`alias-same-name`**: `mrb_alias_method`（src/class.c:3674）の先頭が `if (a == b) return;` で、
  探す前・凍結を見る前に戻る。ただし呼び手（`mrb_mod_alias`、`OP_ALIAS`、vm.c:4664）はそのあと
  `mrb_method_added` を呼ぶので、フックは飛ぶ。`Vm::alias_method` の先頭で同じ名前なら
  `method_added` だけ呼んで戻るようにした（`vm.rs`、命令ループの外の関数）。凍結したクラスでも
  `alias_method(:x, :x)` は通る（rc2 で確かめた）。公開の `Vm::alias_method` の答えが変わるので CHANGELOG に
  書いた。
- **`match-pos-past-end`**: rc2 の `re_char_to_byte`（mruby-regexp regexp.c:352）は
  `mrb_str_char_to_byte` の答えが長さを超えたら -1（照合しない）。`mrb_str_char_to_byte`
  （src/string.c:746）は最後の文字より先の位置に「末尾 + 1」を返す（`if (i<idx) len++`）ので、文字数より
  大きい位置は照合しない。SabiRuby の `string::char_to_byte` は末尾で止まるので、`"".match("", 1)` が末尾で
  照合して空のパターンが合っていた。`ext_regexp.rs` の `char_to_byte` で、末尾に着いて位置が文字数を超える
  ときだけ `None` にした（文字数は末尾に着いたときだけ数える）。`String#match`、`match?`、`Regexp#match` が
  同じ関数を通る。`"ab".match("", 2)`（位置がちょうど末尾）は両方 MatchData のまま。
- **`undef-method-answer`**: `mrb_mod_undef`（src/class.c:3980 あたり）は `mrb_nil_value()` を返す。
  `Module#undef_method` の戻り値を nil にした（`object.rs`）。
- **`shift-negative`**: `mrb_ary_shift_m`（src/array.c:965）は、数を `mrb_as_int` → `ary_modify_check` →
  空か 0 なら `[]` → 負なら ArgumentError "negative array shift" の順。SabiRuby は `n as usize` で -1 が
  大きな数になり、配列全体を返していた。同じ順に書いた（`array.rs`）。`[].shift(-1)` は `[]`、凍結した
  配列は 0 でも FrozenError（rc2 と同じ）。
- **`fiber-root-status`**: rc2 の根のコンテキストは `init_gc_and_core`（src/state.c:35-37）で 0 埋めされ、
  状態は `MRB_FIBER_CREATED`（include/mruby.h:193、値 0）。状態を書くのは切り替え（mruby-fiber の
  `fiber_switch_context`、fiber.c:188）と、ファイバーが終わって戻るとき（vm.c:929）だけなので、一度も
  戻ってこないうちは "(created)"。SabiRuby は根を `Running` で作っていた。`Created` で作るようにした
  （`vm.rs` の `Vm` の組み立て）。根の状態を見るほかの道（`fiber_switch` の状態の検査、`gc_mark_roots`、
  Task の切り替え）を読み、根が `Created` のまま通る道が無いことを確かめた: 子のファイバーの中にいるなら根は
  切り替えのときに `Resumed` か `Transferred` になっている、`gc_mark_roots` は根を状態によらず積む、
  Task は戻るとき `Running` にする。`Fiber.yield` で戻った後の根は "(resumed)" になる（テストに入れた）。
- **`set-replace-order`**: 直さず、C の `frozen-set` に移した（下の「迷った点」）。

### `native-params`: 全メソッドの `parameters` と `arity`

道具: `tools/rc2-aspec/params.rb`（新しく足した）は、名前のあるクラスとモジュールの全部について、
インスタンスメソッド（public・private・protected）と特異クラスのメソッドを `Owner#name arity parameters`
（特異メソッドは `Owner.name`）の形で書く。`tools/rc2_aspec.sh --compare` がこれも両方で走らせ、両方にある
名前を比べて `target/rc2-aspec/params.diff` に差を残すようにした（Y6 の気づいた点で「道具が要る」と書いたもの）。

**直す前**（`87401ec`、rc2 は utf8 の像。バイトの像でも同じ数）: rc2 1,545 行、SabiRuby 1,330 行、両方にある
1,224 メソッドのうち `parameters` か `arity` が違うのは 614。arity が違うのは 1 つだけ
（`Kernel#define_singleton_method`、README に記録済みの差）。内訳:

| 種類 | 数 | 例（rc2 / SabiRuby） |
|---|---:|---|
| ネイティブ: 必須の引数を rc2 は `:opt` と書く | 355 | `Integer#+` `[[:opt]]` / `[[:req]]`、`Integer#<=>` も同じ |
| ネイティブ: それ以外の書き方の差 | 139 | `Array#first` `[[:opt]]` / `[[:rest]]`（56）、`Array#[]` `[[:opt], [:opt]]` / `[[:req], [:rest]]`（36）、`String#gsub` `[[:rest], [:block]]` / `[[:rest]]`（7）、`Kernel#proc` `[[:block]]` / `[]`（6）、`NameError#name=` `[]` / `[[:rest]]`（2）、`Time.gm` `[[:opt]]×7` / `[[:req], [:rest]]`（4）ほか |
| rc2 の組み込みの Ruby のメソッドで、ブロックの名前の後ろに次の局所変数の名前が付く | 104 | `Array#each` `[[:block, :block, :idx]]` / `[[:block, :block]]` |
| rc2 では Ruby、SabiRuby ではネイティブ | 15 | mruby-io の `p`・`putc`・`print`・`puts`・`printf`・`` ` ``（11、モジュール関数を含む）、`catch`（2）、`BasicObject#!=`、`Class#new` |
| 記録済み | 1 | `Kernel#define_singleton_method` |

ネイティブの 494（355 + 139）を読むと、SabiRuby の `Method#parameters` は、ネイティブについて宣言
（`Aspec`）を見ずに arity から作っていた（`arity >= 0` なら `:req` を arity 個、負なら `:req` と `:rest`、
`ext_method.rs` の `parameters`）。Y6 の表が「`Aspec` が rc2 の宣言と違う」と見ていた `first`・`to_s`・
`center`・`sub` も、`Aspec` は rc2 と同じで、書き方だけが違っていた。rc2 の `mrb_proc_parameters`
（mruby-proc-ext proc.c:168）は、C の関数の RProc に `mrb_proc_set_cfunc_aspec`（include/mruby/proc.h）が
13 ビットに縮めて持たせた宣言（`mrb_proc_compress_aspec`: req・opt は 7、post・key は 3 までで、超えたら
切り詰めて rest を立てる）を戻して読む。RProc は strict でないので req と post も `:opt`、名前は無い。
順は req・opt・rest・post・key、そのあと keyrest、最後に block。宣言が 0 なら `MRB_PROC_NOARG` で `[]`、
縮めたビットが全部 0（noblock だけ）なら `[]`。属性の読み手の RProc は NOARG、書き手は何も持たないので
どちらも `[]`。

直し: `Aspec` に、rc2 の縮めた宣言を表す `compressed()` を足して `arity()` をその上に書き直し（答えは
同じ）、`parameters()`（種類と数の並び、`pub(crate)`）を足した。`Method#parameters`（`UnboundMethod` も
同じ関数）はネイティブ・クロージャのとき、その宣言から作る（`object.rs`、`ext_method.rs`）。

書き方を直した後に残ったネイティブの差は 2 つで、どちらも宣言の差（ブロックの印）だった:
`Class#initialize` は rc2 が `MRB_ARGS_OPT(1)`（`cls_rom_entries`、src/class.c:4673）なのに SabiRuby は
`opt(1) | block`、`Module#initialize` は rc2 が `MRB_ARGS_NONE()`（:4696）なのに `none() | block`。Y2 で
足したときに本体の `"|C&"` からブロックの印を付けていた。rc2 の宣言に合わせた（`object.rs`）。ブロックは
宣言に関係なく渡るので、`Module.new { }` などの答えは変わらない（`rc2_aspec.sh --compare` の「引数の数が
違う呼び出し」1,015 も全部同じのまま）。

**直した後**: 両方にある 1,224 のうち違うのは 120。ネイティブの書き方と宣言の差は 0。残りは上の表の 104（rc2
の組み込みの Ruby のメソッド）、15（Ruby とネイティブ）、1（記録済み）で、どれも `native-params` の根では
ない。片方にしか無いメソッド（rc2 だけ 321 行、SabiRuby だけ 106 行）は比べていない（`method-lists`、
`gem-set` と同じ、ビルドに入っている gem の差）。

104 の根: `mrbc -v` で見ると、`def each(&block)` の irep の局所変数は R1（名前の無いブロックの枠）、R2 `block`、
R3 `idx` で、`lv` は `[0, block, idx]`。`mrb_proc_parameters` はブロックの欄で `lv[i]`（0 なので何も足さない）
と `lv[i+1]`（`block`）を足し、`[[:block, :block]]` になる。rc2 の像の組み込みのメソッドで `idx` まで付くのは、
rc2 がビルドのときに mrblib を C に吐く（`mrbc -B`）からで、`cdump_sym`（mrbgems/mruby-compiler/src/cdump.c:303）
は名前が空の記号（prism の空の定数）を何も書かずに飛ばす。そのため `lv` の配列が 1 つずつ前に詰まり、
`lv[i+1]` が次の局所変数になる。同じソースを rc2 で実行時にコンパイルしても、`.mrb` にして読ませても
`[[:block, :block]]`（`y6b/p3.rb` で確かめた）。SabiRuby は mrblib を `.mrb` から読むので後者と同じ。rc2 の
ビルドの不具合で、表に無い新しい差なので直さず、README の「洗い出しに出ない行」に `params-builtin-lv-shift`
（C の候補、未記録）として 1 行足した。15 は `params-ruby-or-gem`（D）として 1 行。

本との突き合わせ: `book_mruby3/contents` で `parameters` を引くと、codegen.re の Prism のノード名と
porting.re:96（`Proc#parameters` は局所変数の名前に依存する、`mrbc -g` で名前を残す）だけで、ネイティブの
`parameters` の出力を載せている所は無い。`data/code` にも無い。

### `Complex#infinite?`

rc2 の Complex は `Numeric#infinite?`（`num_infinite_p`、常に false）を通る。`Complex(Float::INFINITY, 2)` も
false で、黙って違う値なので C。CRuby は `rect.map(&:abs).max.infinite? ? 1 : nil` の形で、どちらかの部が
無限なら 1、それ以外は nil。SabiRuby は nil だった（Numeric の `infinite?` が Float 以外を全部 nil にする）。
Complex に自分の `infinite?` を足すと `Complex.instance_methods(false)` と持ち主が rc2 と変わるので、Numeric の
`infinite?` の中で Complex を見る形にした（`numeric.rs`）。`Complex(1.5, Float::NAN)` は nil（CRuby も nil）。
README の `Integer#infinite?` の項に Complex を書き足し、`integer_infinite.rb` に 4 行足した（同じ項の記録なので
ファイルを分けなかった）。`finite?` は触っていない（下の気づいた点）。

### C の記録（4 原因）

Y5 と同じ形で、README "Known deviations" の 2026-10-18 の節に 1 項ずつ、`tests/custom/` に 1 本ずつ足した
（期待値は SabiRuby の出力、rc2 の答えは `.rc.out`、頭書きに rc2 の関数と行と CRuby の答え）。どれも
SabiRuby・rc2・CRuby で同じスクリプト（`y6b/c1.rb`）を走らせてから書いた。

- **`time-init-failed`**（`time_init_failed.rb`）: `time_init`（mruby-time time.c:1216）は引数を読んでから、
  受け手のデータを `mrb_free` して `mrb_data_init(self, NULL, &time_type)`（:1228）で空にし、そのあと
  `time_mktime` が値を見て ArgumentError を出す。`nil` のように `mrb_get_args` が断る引数なら空にする前に
  例外になるので、rc2 でも値が残る（テストに両方入れた）。
- **`bigint-bitop-float`**（`bigint_bitop_float.rb`）: 小さい受け手は Float を断るが、幅の広い受け手は
  `mrb_bint_and`・`or`・`xor`（mruby-bigint core/bigint.c:6076-6124）が `mrb_as_bint`（:5517）で 1.5 を 1 に
  する。CRuby は `&`・`|`・`^` を TypeError、`allbits?`・`anybits?`・`nobits?` は false・false・true
  （表の「CRuby TypeError」は 3 つについてだけ正しかったので、README と頭書きは確かめた答えで書いた）。
- **`bigint-pow-mod`**（`bigint_pow_mod.rb`）: `int_powm`（mruby-numeric-ext numeric_ext.c:199）は幅の広い
  受け手・指数・法を `mrb_bint_powm`（bigint.c:5982）に回し、そこは Float の法を `mrb_as_bint` で狭め、法が 1
  のときの x**0 を減らさない。小さい道は Float の法を TypeError、法 1 は 0。CRuby は負の指数を先に見るので
  `(2**70).pow(-1, 1.5)` は RangeError（SabiRuby は TypeError）。頭書きに書いた。
- **`const-get-empty`**（`const_get_empty.rb`）: `mrb_mod_const_get`（src/class.c:4023）は Symbol ならすぐ
  探し、String は `while (off < len)`（:4039）で区切りごとに探すので、空の String では一度も回らず受け手を
  返す。

`compare.py` の `RECORDED` に 4 つ（と `set-replace-order`）を足し、README の項の名前を付けた。

### Y6 が広げた 5 つの規則に README の文をそろえる

- `a[i, len] = v`: 長さが `2**70` の場合を書き足した（rc2 は同じ fallback で `[3, "", 2]`、凍結した配列では
  FrozenError、SabiRuby は長さを先に変換して RangeError）。書く前に CRuby で確かめると、CRuby も RangeError
  だった（最初は「CRuby は `[3, ""]`」と書きかけた。確かめて直した）。
- `Integer#to_s`: 項はもともと「幅の広い Integer は "invalid radix 0"」と書いていたので、`10.to_s(2**70)`、
  `10.inspect(2**70)` を例として足しただけ。
- `Complex#infinite?`: 上。
- 順序でないブロックの `sort`: README の sort の段落に「要素を返すブロック（`{ |*x| x[0] }`）も別の組が渡るので、
  同じ要素の別の並びか同じ例外になる」を足した。
- `delete_prefix`: 項は README ではなく `docs/design/utf8.md` にある。`"あいう".delete_prefix("あ")` が rc2 で
  `""`（バイトの位置 3 を文字の位置として切り、末尾に当たる）を、既にある `"あい"` の例（nil）の隣に足した。

`compare.py` の該当の規則のコメントも「README は名指ししていない」から「Y6b から名指ししている」に直した。

### テスト

- `rc2_sweep_y6b.rb`（期待値は rc2）: A の 6 原因の節と、ネイティブの `parameters`（`1.method(:+)`、`to_s`、
  `first`、`push`、`size`、`center`、`Math.log`、`Class#initialize`、`Module#initialize`、`Hash#keys`、
  `Integer#pow`、属性の読み手と書き手）。アドレスは Regexp を使わずに消す（Y2 と同じ `mask`）。最初は
  `[].method(:each_index)` と Ruby で書いたメソッドの `parameters` も入れたが、前者は上の 104 の行（rc2 の
  組み込み）、後者は rc2 の Ruby のメソッドの名前の読み方の別の癖（`[:key]`、`[:block, :**]`）に当たって
  テストの主題から外れるので外した。`Hash#fetch`（rc2 の組み込みの Ruby）も同じ理由で `Hash#keys` に替えた。
- `rc2_sweep_y6b_regexp.rb`（`# regexp-only:`、期待値は rc2）: `match-pos-past-end` と `"".method(:sub)`。
- C の 4 本: 上。`frozen_set.rb` に `replace(nil)` の 2 行、`integer_infinite.rb` に Complex の 4 行。

### 洗い出しを回し直す

Y6 と同じ手順。`target/rc2-sweep` を `target/rc2-sweep-y6` に移し、`lists`（1 s）、`cases`（9 s）。
`chunks.json` と 4 つの一覧のうち 3 つは前の走行とバイト単位で同じで、`receivers.sabi.tsv` だけが変わった
（この一覧は各メソッドの `parameters` を持つので、ネイティブの `[[:req]]` が `[[:opt]]` などになった。
呼び出しの表には効かない）。`run --timeout 5 --only '^(?!Range:endless$)'`（56 s、restarts 0）。`(1..)` の
raw は Y6 の走行から写した（Y6b の変更は Range のメソッドに触れないので、走らせ直していない）。`compare.py`、
`programs.py run --timeout 60`（71 s、上限に当たったのは前と同じ `test_NOP_empty_loop.rb` の 4 つだけ）と
`compare`。`compare.py diff`（HEAD = Y6 の表）:

```
X1 calls compared / without an answer: 977521 / 4345 (same)
X1 causes: 36 -> 28; 4 new, 12 gone, 1 changed
  gone   A: alias-same-name 10, fiber-root-status 2, match-pos-past-end 6, math-log-order 16,
            set-replace-order 30（C の frozen-set の記録へ）, shift-negative 2, undef-method-answer 4
         C: bigint-bitop-float 12, bigint-pow-mod 8, const-get-empty 4, time-init-failed 22（記録して k へ）
         ?: native-params 2
  new    known: Integer#pow(n, m) with an operand past 64 bits 8, a Time whose initialize fails keeps its
            value 22, an Integer past 64 bits with a Float in &, |, ^ ... 12, const_get("") 4
  changed known: a frozen Set refuses to change 1251 -> 1281 (+30)
X1 methods: 142 -> 135 rows
X2 x2-programs.tsv: 517 -> 523 rows; 6 added, 0 removed, 3 changed
  added: custom の 6 本（C の 4 本は stdout の差、y6b の 2 本は same）
  changed: frozen_set.rb と integer_infinite.rb の出力の長さ（足した行）、vm_task_sleep_switching.rb の 1 バイト（Y0 の揺れ）
X2 x2-asserts.tsv: 250 -> 250 rows; 0 added, 0 removed, 0 changed
```

A は 0、記録の無い C は 0、? は 0。新しく出た原因は無い（new はどれも記録の規則の行）。差のある呼び出しは
32,537 から 32,495（D 16,151、k 16,344）、同じ答えは 944,984 から 945,026。調査（`9375b09`）とは
`X1 causes: 92 -> 28; 14 new, 78 gone, 8 changed`。README の「After the fixes」の表に Y6b の列を足し、
Y6 の残りの表の各行の終わりに Y6b でどうしたかを書き、「洗い出しに出ない行」に 3 行（`params-builtin-lv-shift`、
`params-ruby-or-gem`、`const-get-colons-msg`）を足した。調査の A の表の `frozen`・`bigint-to-float` の「done」
も直した。

### 確かめ

- `tools/mrbtest.sh`（3 ビルド）: utf8 2508 / ok 2346 / ko 51 / crash 100 / skip 11、bytes 2453 / 2269 / 38 /
  90 / 56、no-regexp 2012 / 1981 / 9 / 17 / 5。baseline と同じ。生成した md の差は日付だけなので戻した。
- custom（`cargo test --release --test custom`）: 81 / 79（bytes、`--no-default-features --features
  std,regexp`）/ 73（no-regexp、`--no-default-features --features std,utf8`）、どれも 0 problems。前の 75 / 73 /
  68 から、6 本ずつ（no-regexp は regexp-only の 1 本を除く 5 本）増えた。
- `cargo test --release --workspace`・`cargo test --workspace`（debug、別の target）: どちらも 276 通る。
- `tools/check_no_std.sh`: OK。VM の crate に `unsafe` は足していない（`git diff -- src` に無い）。
- `tools/rc2_aspec.sh --compare`: arity 1,117 のうち違うのは 1（記録済み）、引数の数が違う呼び出し 1,015 は全部
  同じ、`parameters` は上のとおり 1,224 のうち 120。
- `tools/edge_compare.sh`: ファイル 30 passed / 17 failed / 1 runtime_error、assert は pass 858 / fail 33 /
  error 34 / skip 2、`assert.rb` の参考の列は ok 883 / ko 43。前と同じ（生成物の差はアドレス・時間・コミット
  だけ）なので戻した。

### 性能

`tools/bench_ab.sh --core 2`（5 巡、交互）。A は `87401ec`（md5 `cf7714ef…`、最初に建てた基準と同じ）、B は
Y6b の作業ツリー（`6186eaa4…`）、A/A は A の写し 2 つ。同時に 1 つだけ。別の担当の `docker logs` を 2 秒ごとに
読むループが動いていたが、`vmstat` の idle は 98〜99%。

| 走行 | A の合計 | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---:|---:|---:|---:|---:|---:|---:|
| A/B 1（騒がしい: `bm_ao_render` の best と median が 6% 離れた） | 48.0 s | +0.6% | +0.5% | -0.4% | -0.1% | -1.5% | **+0.2%** |
| A/A | 45.3 s | +0.2% | +0.2% | -0.2% | -0.2% | -0.7% | **+0.0%** |
| A/B 2（騒がしい: 14 本で best と median が 1% 超） | 46.1 s | +0.1% | -1.8% | +0.7% | -1.7% | -1.0% | **-0.2%** |

1 本ずつで A/A の幅（-1.6〜+1.8%）を超えたのは、A/B 1 の `bm_ao_render` +5.7% と `bm_mandel_term` -5.8%、
A/B 2 の `vmo_index` -11.8% と `vmo_dispatch` +2.7%。どれも best と median が大きく離れた回なので、9 巡で取り
直した: `bm_ao_render` +0.1%、`bm_mandel_term` -0.2%、`bm_so_mandelbrot` -0.2%。`vmo_*` と `loop_*` は A/B と
A/A を 9 巡ずつ: A/B は `vmo_arith` +0.5%、`vmo_calls` -0.4%、`vmo_dispatch` -1.0%、`vmo_index` -1.6%、
`vmo_objects` -0.3%、`loop_if_branch` -0.9%、`loop_times` +0.7%、`loop_while_add` -0.2%。A/A は -0.5〜+1.1%。
幅の中なので、遅くなったものは無いと判断した。命令ループ（`exec_frames`）と `op_arith` には触っていない
（`vm.rs` の変更は `Vm::alias_method` の先頭の 1 行と、`Vm` を組み立てるときの根の状態だけ）。表はスクラッチパッドの
`y6b/bench/`。

### 迷った点

- **`set-replace-order` を直さずに C に移したこと**: rc2 の `Set#replace` は mrblib の `clear` + `merge`
  （mruby-set mrblib/set.rb:61）で、`Set[1].freeze.replace(nil)` は凍結した Set を `clear` で空にしてから
  `merge` が ArgumentError を出す（rc2 で確かめると、後の Set は `Set[]`）。洗い出しは受け手の後の値も比べる
  ので、例外の種類だけ合わせても行は消えない。rc2 に合わせる＝凍結した Set を空にすることで、これは記録済みの C
  `frozen-set` そのもの。CRuby の答え（ArgumentError で Set はそのまま）を採るには、SabiRuby の `clear` が先に
  凍結を断る以上、gem の Ruby の `replace` をネイティブで置き換えるしかなく、「gem が Ruby で定義するメソッドを
  ネイティブで置き換えない」に反する。方針で決まると判断し、FrozenError のまま `frozen-set` の項に書き足し、
  原因の名前は残して見立てを C、`RECORDED` で `frozen-set` と同じ項の名前にした。Y6 が A とした理由（rc2 と
  CRuby の例外が同じ）は、rc2 が Set を空にしていることを見ていなかった。計画の「A の 7 原因を rc2 に合わせる」
  とは違う結果なので報告に書く。
- **Y6 の表の「`Aspec` が rc2 の宣言と違う」**: 全部を比べると、`first`・`to_s`・`center`・`sub` の宣言は
  rc2 と同じで、`parameters` の作り方が違っていただけだった。宣言そのものの差は `Class#initialize` と
  `Module#initialize` のブロックの印の 2 つ。
- **`parameters` の差の残り 120 を直さなかったこと**: どれもネイティブの `parameters` ではない（rc2 のビルドの
  不具合、Ruby とネイティブの違い、記録済み）。計画の箇条は「ネイティブの書き方と宣言を rc2 に合わせる」なので、
  表に行を足すだけにした。
- **`const-get-colons-msg`**: `const_get("::")` の文言（rc2 は空の名前、SabiRuby は `'::'`）。B だが、洗い出しの
  値に無い新しい差なので、計画どおり直さずに表に 1 行足した。

### 本で触れている所

- opcodes.re:5850 あたり（`ALIAS`）: 「元の名前のメソッドが見つからなければ NameError」。`alias x x`（同じ名前）は
  rc2 の `mrb_alias_method` が探す前に戻るので NameError にならない。本の文と食い違いはしないが、本が触れていない
  例外になる。
- vm.re:839（`Fiber.current` は根のコンテキストに `RFiber` を作る）とファイバーの状態の図（`fiber_states`）:
  根の状態が最初は CREATED で、切り替えで戻るまで "(created)" と書かれることは本に無い。本のプログラム
  （`vm_fiber2.rb`）は根を inspect しないので出力は変わらない。
- `Method#parameters`: 本はネイティブの出力を載せていない（上）。porting.re:96 は Ruby のメソッドの名前の話。
- 洗い出しの X2 で、本のプログラムの差は 18 本のまま（Y6 と同じ）。

## 気づいた点（Y6b）

- **`Complex#finite?`** は rc2 も SabiRuby も `Complex(Float::INFINITY, 2).finite?` が true（`num_finite_p`）、
  CRuby は false。rc2 と同じなので洗い出しには出ないが、黙って違う値で、Y6b の後の SabiRuby では `infinite?` が 1
  なのに `finite?` が true と、SabiRuby の中で食い違う。属する先: VM（C の候補、著者の判断）。
- **rc2 の組み込みの Ruby のメソッドの `parameters`**（`params-builtin-lv-shift`、104 メソッド）: rc2 の
  `cdump_sym`（mrbgems/mruby-compiler/src/cdump.c:303）が名前の空の記号を飛ばして `lv` を詰める。`parameters`
  だけでなく、組み込みのメソッドの中の `binding.local_variables` のように `lv` を読むものにも出るはず（確かめて
  いない）。属する先: 本家の不具合の候補（本の porting.re:96 の近く、`lv` と `mrbc -B` の話の素材にもなる）。
- **rc2 では Ruby、SabiRuby ではネイティブのメソッド**（`catch`、`BasicObject#!=`、`Class#new`）。`catch` は
  mruby-catch の Ruby（SabiRuby も `catch.mrb` を読む）なのに SabiRuby はネイティブも持つ。「gem が Ruby で定義
  するメソッドをネイティブで置き換えない」に当たるかもしれない（理由は読んでいない）。属する先: VM（設計の確認）。
- **rc2 の Ruby で書いたメソッドの `parameters` のキーワードの名前**: `def f(a, b = 1, *c, d, e:, f: 2, **g, &h)` で
  rc2 は `[:key, :h], [:key, :e]` のように名前がずれる（SabiRuby も同じ答え。同じ読み方を写しているため）。rc2 と
  同じなので差ではない。属する先: 本家の不具合の候補。
- **`const_get("::")` の文言**（`const-get-colons-msg`、B、上）。属する先: VM（B、著者の判断で次の段）。
- **`(1..)` の受け手は走らせ直していない**: Y6b の変更は Range のメソッドに触れないと判断して Y6 の raw を写した。
  `Array#shift` などを `(1..)` の答えの中で呼ぶ道（`first(n)` の中など）があれば変わりうる。属する先: 道具。

## Y7. `OP_ADDILV`／`OP_SUBILV` の速い道（と、その前の `Complex#finite?`）

基準: main `79fa10e`（Y6b の後、計画に Y6b の結果を書いたコミット）。

### `Complex#finite?`（Y6b の最後の箇条、Y7 の前に別のコミット）

Y6b の気づいた点のとおり、rc2 の Complex は `Numeric#finite?`（`num_finite_p`、src/numeric.c:2448、
`return mrb_true_value()`）を通り、`Complex(Float::INFINITY, 2).finite?` も true。Y6b で `infinite?` を CRuby の
1 にしたので、SabiRuby の中で `infinite?` が 1 なのに `finite?` が true と食い違っていた。CRuby 3.2.6 は
`rect.all?(&:finite?)` の形で、`Complex(Float::INFINITY, 2)`・`Complex(1, -Float::INFINITY)`・
`Complex(1.5, Float::NAN)`・`Complex(Float::NAN, 1)` が false、`Complex(1, 2)` が true（スクラッチパッドの
`fin.rb` で確かめた）。`infinite?` と同じく、Complex に自分のメソッドを足すと持ち主が rc2 と変わるので、
Numeric の `finite?` の中で Complex を見る形にした（`numeric.rs`、部のどちらかが有限でない Float なら false）。
NaN の部は `infinite?` では nil（無限ではない）、`finite?` では false（有限でもない）で、CRuby と同じ。

README の `Integer#infinite?` の項に `finite?` を書き足し、`integer_infinite.rb` に 4 行足した（期待値は
SabiRuby、rc2 の答えは `.rc.out`。rc2 は 4 つとも true。`kishima/mruby:4.1.0-rc2-utf8` でも出力は 1 字も
違わない）。洗い出しの Complex の受け手は有限なので（`compare.py` の `infinite?` の規則のコメント）、表は
変わらないはず（確かめは下の「確かめ」）。

最初に `cargo build --release` だけで建てて古い `sabiruby` で走らせ、答えが変わらないと見誤った。CLI は
`sabiruby-cli` の crate なので、`-p sabiruby-cli`（か `--workspace`）で建てる。

### `OP_ADDILV`／`OP_SUBILV` の速い道

今（`src/vm.rs` の `Op::Addilv | Op::Subilv` の腕）は、受け手が Integer か Float なら作業用のレジスタ `b`・`b + 1` に
写して毎回 `op_arith` を呼び、結果を `a` に写し戻していた。`op_arith` の中で `mid` を `plus`・`minus`・`mul` と順に
比べて演算を決め直すので、命令が既に言っていることをもう一度調べている（`arith!` の頭のコメントと同じ話）。
rc2 の `OP_MATHILV`（src/vm.c:4154-4197）は、Integer を `mrb_int_add_overflow` でその場で足し、あふれたときだけ
`OP_MATH_OVERFLOW_INT`、Float もその場（`OP_MATHILV_CASE_FLOAT`）、それ以外は `mrb_funcall_argv` で送る。

直し: 腕を `Op::Addilv` と `Op::Subilv` に分け、`arith!` と同じ形のマクロ `mathilv!($int, $f, $mid)` にした。
`stack[base + a]` が Integer なら `checked_add`／`checked_sub` で計算し、あふれたら `#[cold] #[inline(never)]` の
`op_mathilv_overflow(base, a, b, c, mid)` に回す（今までと同じく作業用のレジスタに写して `op_arith` を呼び、
写し戻す。幅の広い Integer になる）。Float はその場で `p + c as f64`（`float_op` の `+`・`-` と同じ式）。それ以外は
初め、今までどおり腕の中で `self.funcall(recv, ...)` を呼んでいた（下の「1 つ目」）。答えは変えていない
（rc2 も SabiRuby の `Addi` も、速い道では `Integer#+` の再定義を見ない）。

**1 つ目（`b`、腕の中に `funcall` を残した形）は遅くなった。** A = `6ef6070`（`Complex#finite?` のコミット、
md5 `32a57a27…`）、`--core 2`、5 巡: 合計 **+6.5%**。`loop_while_add` -10.7%、`vmo_arith` -12.0%、`vmo_dispatch`
-15.1% と `i += 1` の多いものは速くなったのに、`loop_if_branch` **+52.2%**、`call_fiber` +33.0%、`vmo_index`
+32.1%、`bm_ao_render` +22.8%、`call_kwargs` +18.3%、`mem_short_lived` +20.2% と、ほかが大きく遅くなった（best と
median の差はどれも 1〜2% で、騒がしい回ではない）。`exec_frames` は 0xcbf1 から 0xcd4f（+350 バイト）。Y2・Y4 の
教訓どおり、被演算子（`recv`）を値で渡して呼ぶ枝が熱い関数の中にあると、関数全体の割り当てが変わって別の熱い道が
遅くなる、の形と見た（`perf annotate` までは見ていない）。今までの腕にも `funcall(recv, …)` はあったが、それは
Integer と Float も同じ `op_arith` の呼び出しに寄せた後の 2 択だった。

**2 つ目（`b2`、採った形）**: それ以外の受け手も `#[cold] #[inline(never)]` の `op_mathilv_send(base + a, c, mid)` に
出し、関数がレジスタから受け手を読み直して `funcall` し、答えを書き戻す（`op_add_str` と同じ形）。腕に残るのは
Integer の足し算と、Float の足し算と、冷たい関数の呼び出し 2 つ。`exec_frames` は 0xcc3b（A より +74 バイト）、
`op_mathilv_send` 0x110、`op_mathilv_overflow` 0x12a。

テスト: `tests/custom/addilv_fast_path.rb`（期待値は rc2、`.expected` は `.rc.out`。utf8 の像でも同じ出力）。
始めの値は全部 `v(x)` を通す。最初に `i = 9223372036854775806; i += 1` と書いたら、4.1 のコンパイラが
`LOADL 9223372036854775807` に畳んでいて ADDILV が出ていなかった（`.dump` で気づいた）。`begin` の中の `s += 1` も
融合されない（`MOVE`・`ADDI`・`MOVE`）ので、例外になるものはメソッド `add1(x)` の中に置いた。`.dump` に ADDILV／
SUBILV が 27 個。中身: 最大の Integer を越える・最小を下回る（`+= 1`、`+= 200`、`-= 201`）、その後の幅の広い
Integer、ループの中で越える、255 の即値、Float（無限大、NaN、`0.1 + 2`）、`+`・`-` を持つ自分のクラス（受け手を
返す・別の値を返す）、Rational、Complex、String（TypeError）、nil（NoMethodError）、String の子クラスの `+`、
`Integer#+` と `Float#-` の再定義（速い道は見ない、`2**64` の受け手は送るので再定義が答える）。直す前の
`6ef6070` でも同じ出力。

### 確かめ（`b2` の作業ツリー）

- custom 82 / 80（bytes）/ 74（no-regexp）、どれも 0 problems（前の 81 / 79 / 73 から 1 本ずつ）。
- `cargo test --release --workspace`・`cargo test --workspace`（debug、別の target）: どちらも 276 通る。
- `tools/mrbtest.sh`（3 ビルド）: baseline と同じ（utf8 2346、bytes 2269、no-regexp 1981。生成した md の差は日付
  だけなので戻した）。`check_no_std.sh` OK。`git diff -- src` に `unsafe` は無い。
- 洗い出し: Y6b と同じ手順（`target/rc2-sweep` を `target/rc2-sweep-y6b` に移し、`lists`・`cases` は 5 つとも前と
  バイト単位で同じ、`run --timeout 5 --only '^(?!Range:endless$)'` 44 s・restarts 0、`(1..)` の raw は Y6b から写した、
  `compare.py`、`programs.py run --timeout 60` と `compare`）。`compare.py diff`（HEAD = Y6b の表）:
  `X1 causes: 28 -> 28; 0 new, 0 gone, 0 changed`、`X1 methods: 135 -> 135 rows; 0 added, removed or changed`。X2 は
  `addilv_fast_path.rb`（same）が増え、`integer_infinite.rb` の長さと差の行（`Complex#finite?` の 3 行、記録した差）、
  `vm_task_sleep_switching.rb` の 1 バイト（Y0 の揺れ）。`Complex#finite?` も洗い出しの表には出ない（受け手の
  Complex は有限）。生成した表は戻した。
- `tools/edge_compare.sh`: ファイル 30 / 17 / 1、assert 858 / 33 / 34 / 2、参考の列 883 / 43 で変わらず。生成物は
  戻した。

### 性能

`tools/bench_ab.sh --core 2`、5 巡、同時に 1 つだけ。回す前に `pgrep` で他の cargo・ベンチが無いこと、`vmstat` の
idle 99〜100% を見た（別の担当の `docker logs` を 2 秒ごとに読むループは Y6b と同じく動いていた）。A = `6ef6070`、
A0 = `11e595c`（md5 `56c61da6…`、Y4 と同じバイナリ）、B = `b2`（`57d73636…`）。

| 走行 | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---:|---:|---:|---:|---:|---:|
| A/B（`6ef6070` → `b2`） | -4.0% | +0.0% | -12.0% | -0.9% | -1.3% | **-4.8%** |
| A/A（`6ef6070` の写し 2 つ） | +0.3% | +0.3% | +0.3% | +0.6% | +0.5% | **+0.3%** |
| A0/B（`11e595c` → `b2`） | -3.5% | -0.3% | -10.9% | -0.4% | -1.3% | **-4.2%** |
| 参考: A/B（`6ef6070` → `b`、捨てた形） | +7.4% | +10.2% | -4.5% | +15.0% | +15.1% | **+6.5%** |

| 1 本ずつ | A/B | A/A | A0/B |
|---|---:|---:|---:|
| `loop_while_add` | -13.3% | +0.0% | -11.5% |
| `loop_times` | -5.8% | +0.1% | -3.9% |
| `loop_if_branch` | -3.8% | -0.8% | -2.6% |
| `vmo_arith` | -14.1% | +0.5% | -13.0% |
| `vmo_dispatch` | -18.9% | +0.3% | -18.0% |
| `vm_optimization_bench` | -8.8% | +0.7% | -8.2% |
| `bm_so_mandelbrot` | -3.7% | +1.1% | -3.2% |
| **`call_fiber`** | **+2.6%** | -0.6% | **+2.0%** |
| **`bm_so_lists`** | **+2.3%** | -0.1% | **+1.9%** |

A/A の 1 本ずつの幅は -0.8〜+1.8%（最大は `vmo_objects`）。`i += 1` の多いループは速くなった。Y4 から残っていた
`loop_*` の `11e595c` に対する +1.4〜2.1% は、-2.6〜-11.5% になって消えた（`loop_if_branch` は ADDILV を通らない
と思っていたが -2.6%。`loop_if_branch` の本体にも `i += 1` がある）。

ただし `call_fiber` と `bm_so_lists` が、A/B と A0/B の両方で +1.9〜+2.6% と A/A の幅を超えた。9 巡で取り直した:
`call_fiber` A/B +1.7%（A/A -0.2%）、`bm_so_lists` A/B +2.3%（A/A +0.4%）。どちらも 1 回の反復に ADDILV が 1 つ
あるだけで（`call_fiber` は `Fiber#resume` のループ）、ADDILV そのものの重さではない。`perf record -e cpu-clock`
（この WSL では cycles などのハードウェアの事象が使えない）で `call_fiber` と `bm_so_lists` を両方のバイナリで
見ると、熱い番地はどちらも `exec_frames` の中の同じ並びで、番地が 0x140 ずれているだけ、割合もほぼ同じ（最も
熱い番地は両方 64 バイト境界からの位置が 0x3a で同じ）。原因はここまでで分からない。計画と指示の「A/A の幅を
超えて遅くなったら止める」に従い、いったんコミットせずに止めて報告した。

**著者の判断（2026-10-18、本体経由）**: 全体の得（A/B -4.8%、命令ループ -12.0%）が大きいので、今の形のまま
コミットする。`call_fiber`（5 巡 +2.6%・A0/B +2.0%・9 巡 +1.7%、A/A -0.6%・-0.2%）と `bm_so_lists`（5 巡 +2.3%・
A0/B +1.9%・9 巡 +2.3%、A/A -0.1%・+0.4%）は配置による差として記録する（確かめてはいない）。perf で見たのは上の
とおり、熱い番地の並びと割合が同じで番地がずれただけ、ということまで。まだ試していない切り分けの手順は、Y2 と
同じく: (1) `objdump` で `exec_frames` の Send・`Fiber#resume`／`yield` の切り替えの腕を A と B で並べ、命令の
違い（スタックへの書き出しの増減）を見る、(2) `op_mathilv_overflow` と `op_mathilv_send` を片方ずつ腕に戻して
A/B を取る、(3) `Op::Addilv` と `Op::Subilv` を 1 つの腕にまとめて（`mid` を腕の中で選ぶ）A/B を取る。計画書の
Y8 に「各段のベンチで `11e595c` とも比べ、この 2 本の差が続くかを見る」を足した。

洗い出しの置き場所: `target/rc2-sweep` は Y7 の作業ツリー（このコミットと同じコード）で走らせた最新の走行なので、
そのまま残した。Y6b の走行は `target/rc2-sweep-y6b` に置いたまま（Y5・Y6 の `rc2-sweep-y5`・`rc2-sweep-y6` と同じ
並べ方）。戻さなかった理由: 戻すと、次の段の `compare.py diff` の前の走行が HEAD より 1 つ古くなる。
表はスクラッチパッドの `y7/bench/`（`y7-ab.tsv` が捨てた形、
`y7-ab-v2.tsv`、`y7-aa.tsv`、`y7-ab0-v2.tsv`、`y7-9-*`）。

## Y8. ほかの速い道（Y8a〜Y8d）

基準: main `83ede9c`（Y7）。各段を別のコミットにし、各段で `tools/bench_ab.sh --core 2`（5 巡、交互）の A/B（A = 直前の
コミット）と A/A（A の写し 2 つ）、それに A0 = `11e595c`（md5 `56c61da6…`、Y4・Y7 と同じバイナリ）に対する A0/B を
取った。Y7 の残り（`call_fiber`・`bm_so_lists` が `11e595c` に対して +1.9〜2.6%）が続くかを、A0/B で見る。前後の
バイナリはスクラッチパッドの `y8/bin/` に写し、別の `CARGO_TARGET_DIR`（11e595c は `target/y8-a0`、試しの形は
`target/y8-al-base`・`target/y8-wt`）で建てた。

## Y8a. 可視性をメソッドキャッシュに持たせる

### 読んだこと

`op_send_vis`（`src/vm.rs`）は、明示の受け手がある SEND（`explicit`）でキャッシュに当たっても、毎回
`method_vis(owner, mid)` を呼んでいた。計画書には「スーパークラスの鎖を歩く」とあるが、渡すのは見つかった
`owner` なので、実際には最初の節で答えが出る（`owner` は `find_method_entry` が答えた節で、その表に `mid` がある）。
それでも 1 回ごとに `heap.class(owner)`、`table_owner`（include class なら module の、prepend されていれば origin の
表）、`methods.contains_key`、`vis.get` と、HashMap を 2 回引く。rc2 は `m.flags` に可視性を持つので、キャッシュに
当たれば検査は flags を見るだけ。

計画書の注意の 1 つ目、`set_visibility` が `method_serial` を上げるか: `set_visibility` は表に書くとき
`heap.class_mut(t)` を通り、`Heap::class_mut`（`src/object.rs:883`）が `method_serial += 1` する。可視性を変える
ほかの所（`Vm::add_method` の `vis.insert`／`remove`、`ext_metaprog.rs` の `remove_method`・`undef`、
`object.rs:635` の singleton の写し、`prepend_module` の表の移し）も全部 `class_mut` を通る。`ObjKind::Class` を
`get_mut` で直接書き換える所は無い（grep で確かめた）。

注意の 2 つ目、`method_vis` と `find_method_entry` の `origin` の扱い: 2 つとも「`origin` を持つ節（prepend された
クラス自身）は飛ばし、`table_owner(x)` の表を見る」で、違いは `Method::Undef` を見つけたときだけ（`find_method_entry`
は `Some(Undef)` を返してラッパが `None` にし、`method_vis` はその節の可視性を返す）。キャッシュに入るのは
`find_method_ref` が見つけたもの（Undef は `None`）なので、見つかった `owner` から引く形（`vis_of_found(owner, mid)`
= `table_owner(owner)` の `vis.get(mid)`）は、`method_vis(owner, mid)` が最初の節で返す値と同じになる。
`method_vis` 自体はほかの呼び手（`alias`、`instance_methods` など、クラスから引くもの）のために残した。

### 直し

`MethodCacheLine` に `vis: Vis` を足し、外れたときに `vis_of_found` で埋める（見つからなかったときは `Public`）。
`find_method_cached_vis` が `(found, vis)` を返し、`find_method_cached` はその `found` だけを返す。`op_send_vis` は
`explicit && !is_super && vis != Vis::Public` のときだけ `#[cold] #[inline(never)]` の `op_send_vis_check` を呼ぶ
（private を断り、protected は呼び手の `self` が持ち主の kind でなければ断る。受け手はレジスタから読み直す）。
文言は前と同じ。答えは変えていない。

### テスト

`tests/custom/method_cache_vis.rb`（期待値は rc2、`.expected` は `.rc.out`。utf8 の像でも出力は同じ）。どの呼び出しも
同じ形で 200 回呼んでキャッシュに載せてから変える: `private`／`public`／`protected` の付け外し（持ち主で）、
継承したメソッドを子で `private`（子に写る、親は public のまま）、`remove_method` の後に親で `private`、prepend した
module の `m` の `private`／`public`、module の `m` を消した後の origin の `C#m` の `private`／`public`、
`module_function`（インスタンスのメソッドは private、特異メソッドは public）、`protected` を同じクラス・子クラスの
インスタンスから・外から、子で `public` にした写し、特異メソッドの `private`。直す前の `83ede9c` でも同じ出力。

書いている途中で、表に無い差を 1 つ見つけた: prepend したクラスで、module と同じ名前のメソッドを `private :m`
すると、rc2 は `NoMethodError: no superclass method 'm'`、SabiRuby と CRuby 3.2.6 は `"P#m+C#m"`。rc2 の
`mrb_mod_visibility`（src/class.c:2523）は `mrb_method_search(mrb, c, mid)` でクラスから引くので prepend した `P#m` を
先に見つけ、それを origin の表（`C#m` のある所）に写すので、`C#m` が private な `P#m` に置き換わる。計画の
「直さずに表に 1 行足す」に従い、`docs/verification/rc2-sweep/README.md` の「Rows the sweep cannot show」に
`vis-prepended-copy` を足した（見立ては C の候補、記録はまだ。著者に任せる）。テストからはこの形を外した。
もう 1 つ、`private_class_method` は rc2 の像に無い（NoMethodError）ので、テストに入れなかった。

### 性能: 最初の形は `gc_churn` を遅くした

**1 つ目（`found` を `Option<(MethodRef, ObjId, Vis)>` に広げた形）**: A/B 全体 -2.5%、`bm_ao_render` -6.9%、
`app_robot` -3.9%、`vmo_calls` -3.6% と狙った所は速くなったが、`bm_so_mandelbrot` +2.6%、`bm_mandel_term` +2.6%、
`gc_churn` +2.2% が A/A（-1.4〜+1.5%）を超えた。9 巡で取り直しても `bm_so_mandelbrot` +2.5%（A/A +0.8%）、`gc_churn`
+2.5%（A/A +0.3%）。この形では `find_method_cached_vis` が 0x28d バイトの独立した関数になり、前は全部の呼び手に
インライン化されていた `find_method_cached` がその呼び出しになっていた。`#[inline(always)]`（と
`find_method_cached` に `#[inline]`）で前と同じくインライン化させた（`nm` で、増えた関数は `op_send_vis_check` と
`vis_of_found` だけになった）が、9 巡で `bm_so_mandelbrot` +2.1%、`gc_churn` +0.4〜+2.2% が残った。

切り分け:
- 全関数を 64 バイト境界にそろえた建て方（`RUSTFLAGS="-C llvm-args=-align-all-functions=6"`、A と B の両方、Y2 と
  同じ）では `bm_so_mandelbrot` -0.4% で消えた。`bm_so_mandelbrot` は `exec_frames` の置き場所（A は 64 バイト境界から
  0x10、B は 0x30）によるもの。`gc_churn` はそろえても +1.5%・+2.4% で残った。
- `gc_churn`（`i = 0; while i < 1_000_000; a = [i, "x" * 10, {k: i}]; i += 1; end`）はキャッシュに 1,000,055 回
  引いて外れは 55 回（一時的に数えた形で測った。コミットしていない）。外れたときの `vis_of_found` は効いていない。
- 片方ずつ戻した: (v-a) キャッシュは `Vis` を持つが `op_send_vis` は前どおり `method_vis` を呼ぶ形: `gc_churn` +1.1%・
  +3.1%。(v-b) キャッシュはそのままで、検査だけ冷たい関数に出した形: `gc_churn` +0.3%・-0.1%、ただし `bm_ao_render`
  -0.9%、`call_args` -1.8% と得もほとんど無い。得はキャッシュに可視性を持たせた所から来て、損は `found` を広げた所から
  来ている。
- **2 つ目（採った形、v-c）**: `found` は前の `Option<(MethodRef, ObjId)>` のまま、`vis` を行の別のフィールドにした。
  行は 40 バイトから 48 バイトになるが、`find_method_cached` の答えの型は変わらない。9 巡 2 回で `gc_churn` -0.7%・
  -0.5%（同じ時の A/A +1.5%・+1.2%、1 つ目の形は +1.9%・+1.4%）、5 巡で `bm_ao_render` -7.8%、`call_args` -7.0%、
  `bm_so_mandelbrot` +0.2%。なぜ `found` を広げると `gc_churn` だけが遅くなるのかは確かめていない（`perf` は、この
  WSL ではバイナリの記号が引けず、番地を `nm` に当てる形もずれていて読めなかった）。

`exec_frames` の大きさは 0xcc3b で A と同じ（`op_send_vis` は 0x1f28 から 0x1e15）。

| 走行（採った形、`e4d3818a…`） | whole program | data structures | instruction loop | calls | memory | **all** |
|---|---:|---:|---:|---:|---:|---:|
| A/B（`83ede9c` → Y8a） | -3.7% | -6.6% | -1.0% | -4.8% | -5.6% | **-3.7%** |
| A/A（`83ede9c` の写し 2 つ） | -0.1% | +0.4% | +0.0% | -0.3% | -0.0% | **-0.1%** |
| A0/B（`11e595c` → Y8a） | -7.3% | -5.5% | -11.9% | -5.3% | -6.3% | **-7.8%** |
| 参考: A/B（1 つ目の形、インライン化の前） | -2.5% | -4.9% | -0.0% | -4.2% | -4.3% | **-2.5%** |

| 1 本ずつ | A/B | A/A | A0/B |
|---|---:|---:|---:|
| `bm_ao_render` | -8.5% | -0.4% | -7.8% |
| `app_robot` | -6.2% | +0.8% | -7.6% |
| `vmo_calls` | -4.3% | -1.0% | -5.0% |
| `call_args` | -8.3% | +0.8% | -9.0% |
| `bm_so_mandelbrot` | -0.2% | +0.2% | -5.0% |
| `gc_churn` | +0.2% | +0.2% | -0.9% |
| **`call_fiber`** | -9.6% | -0.1% | **-7.8%** |
| **`bm_so_lists`** | -13.1% | +0.2% | **-9.4%** |

A/A の 1 本ずつの幅は -1.4〜+3.6%（最大は `vmo_objects`）。A/B で遅くなったものは無い（最大 `gc_churn` +0.2%）。
best と median の差は最大 3.5%。Y7 の残りの `call_fiber`・`bm_so_lists` は `11e595c` に対して -7.8%・-9.4% になった。
`op_send_vis` を通る SEND の多いもの（`call_fiber` は `Fiber#resume`、`bm_so_lists` は Array の `push`／`shift` など）
なので、この段の得で上書きされたと見ている（配置の差そのものが消えたかは、この段の後では分からない）。

途中で機械が「遅い状態」（`bm_ao_render` が 5.4 s から 6.4 s）に入り、その間の A/A は best と median の差が 8〜28% で
読めなかった。静かに戻ってから取り直した表だけを上に載せた。表はスクラッチパッドの `y8/bench/`（採った形は
`y8avc-{ab,aa,a0}.tsv`、1 つ目の形は `y8a-*`・`y8af-*`、切り分けは `var-*`・`var2-*`・`var3-*`・`al-*`）。

### 確かめ

- `tools/mrbtest.sh`（3 ビルド）: utf8 2346、bytes 2269、no-regexp 1981（baseline と同じ。md の差は日付だけで戻した）。
- custom（`cargo test --release --test custom`、3 ビルド）: 83 / 81（bytes）/ 75（no-regexp）、どれも 0 problems。
- `cargo test --release --workspace`・`cargo test --workspace`（debug、`target/y8-debug`）: どちらも 276 通る。
- `tools/check_no_std.sh`: OK。`git diff -- src` に `unsafe` は無い。
