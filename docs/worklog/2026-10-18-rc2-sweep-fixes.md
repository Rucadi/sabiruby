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
