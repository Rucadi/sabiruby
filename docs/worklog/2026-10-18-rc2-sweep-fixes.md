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
