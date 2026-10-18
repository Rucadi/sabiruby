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
