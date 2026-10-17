# rc2-sweep の差を直す（A・B 全部と C の記録）と、洗い出しの道具を常用の形に

作成 2026-10-18。差の一覧は `../verification/rc2-sweep/README.md`（原因ごとの表、見立て A／B／C／D／?）、経緯は `../worklog/2026-10-18-rc2-sweep.md`。方針は `../design/compatibility.md`。正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2-utf8`、文字列がバイトの比べは `kishima/mruby:4.1.0-rc2`。ソースは `/home/kishima/book/ref/mruby` の 4.1.0-rc2。**このクローンの HEAD を動かさない**。ほかの repo にファイルを作らない・消さない）。基準は main `9375b09`。

著者の判断（2026-10-18、選択肢で）:
- 直す線: **A と B を全部直し、C を記録する**。D は何もしない。
- `kw-dropped`（`[1].first(k: 1)`、26 メソッド）: **SabiRuby のまま**（TypeError）。C として記録する。
- `fill-empty-range`（`[].fill(0, 1..3)`）: **CRuby の答え** `[nil, 0, 0, 0]` にする。rc2 は未初期化のメモリを読むので、C として記録する。
- `int-div-nonfinite`: **両方とも rc2 に合わせる**（NaN・Infinity で割ると RangeError、`2**70` で割っても RangeError）。rc2 の `int_idiv`（`src/numeric.c:299`）は、受け手が小さい Integer のとき、割る数を `mrb_as_int` で機械の整数にしてから割る。NaN・Infinity・`2**70` は機械の整数にならないので RangeError になり、1.5 は 1 に切り捨てられる（C の `int-div-float`）。根は同じ。rc2 が例外を出す所は合わせ、黙って違う値を返す所（`7.div(1.5)` が 7）は SabiRuby のまま残す、という線になる。この線は著者の判断として `../design/compatibility.md`「mruby's limits are the specification」に記録した（mruby の限界は仕様。mruby との互換の優先度を下げるときに考え直す）。
- `bigint-float`（`0.quo(2**70)`）: **SabiRuby のまま**（`(0/1)`）。C として記録する。rc2 の `(2**70).quo(3)` が `(0/1)` になる件も一緒に書く。
- `method-owner`: **持ち主の差を全部 rc2 に合わせる**（`1.method(:between?).owner` が Comparable になるように。rc2-leftovers9 V1-1 の表の残り全部）。
- 道具: Y0 のとおり、常用の形にする。

## Y0. 洗い出しの道具を常用の形に（最初にやる）

この後の段の確かめに使うので、最初に済ませる。

- **比べる相手を変えられるようにする**: `tools/rc2_sweep/sweep.py` の `IMG` と `programs.py` の `IMAGES` は rc2 の像の名前を直接書いている。これを引数（`--image`、バイトの像は `--bytes-image`）で渡せるようにし、既定値は今の 2 つにする。4.1 正式版や master と比べるとき、コードを変えずに済むようにするため。
- **前回との差を出す**: `compare.py diff <前の x1-causes.tsv>`（名前は実装に任せる）を足す。原因ごとの呼び出しの数の増減、新しく出た原因、消えた原因を出す。比べる元は、コミットしてある `docs/verification/rc2-sweep/x1-causes.tsv` と `x2-causes.tsv`。X2 は `x2-programs.tsv`・`x2-asserts.tsv` の行の増減でよい。
- **数を `docs/numbers.md` に載せる**: 道具の数（`--timeout`、`--jobs` の既定（CPU の 1/4）、`--mem` の既定（空きメモリの半分 ÷ 同時に走るプロセスの数）、`SW_YIELD_MAX = 9**3`、3 個の引数のラテン方格）を、既定値・どこで変えるか・出どころ付きで 1 行ずつ。出どころは worklog に書いてある。
- **いつ回すか**: `docs/verification/rc2-sweep/README.md` の Tools と `docs/README.md` に書く。回すのは段階の終わりと、本家の新しいバージョンへの追従のとき（全体で数十分かかり、Docker を使うので、毎コミットでは回さない）。所要時間は Y0 の確かめで測った値を書く。
- **確かめ**: 直しのない今の main で全体を回し直し、`diff` が「変化なし」になる（未走行の `(1..)` は除く）。所要時間を worklog に。

## Y1. 危ないもの（A のうち先に）

- `crash`: `"abc".bytesplice(0, 1)` の panic（`src/builtins/string.rs:541`）。ここでは panic を止めるだけにして、rc2 と同じ答えにするのは Y2 の `bytesplice` で行う。
- `setmcnst-frozen`: 凍結したクラスへの `A::Y = 2` を FrozenError にする（`OP_SETMCNST`。本 opcodes.re:2056、`data/code/exp_GETCONST_frozen.rb`）。
- `tdef-no-target`: `1.instance_eval { def x; end }` を TypeError "no class/module to add method" にする。`alias`・`undef` も同じ（本 opcodes.re:5511、`data/code/exp_TDEF_target_class.rb`）。

## Y2. A の残り（rc2 に合わせる）

README の「A — follow rc2」の表の残り全部と、`int-div-nonfinite`（両方）。大きさが中のものは 2 つある。
- `bytesplice`: rc2 は 2 つの引数を (range, str) と読み、境界を見る前に変換する。文言の綴りの誤り "arumgnts" も rc2 のまま写す。方針「文言は本家に合わせる」による。
- `bigint-to-float`: `Math` などの Integer → Float の変換。
- `string-add-redefine` は `OP_ADD` の速い道（String どうし）に触る。下の「性能」を見ること。
- `fill-empty-range` は rc2 ではなく CRuby の答えにする（著者の判断）。C の記録は Y5 で。

## Y3. メソッドの持ち主を rc2 に合わせる

- 持ち主の差の一覧を、まず両方の VM から取り直して数える（`x1-one-side.tsv` と rc2-leftovers9 V1-1 の表。`Integer#<` と `Numeric#<`、`between?` の Comparable など）。数と内訳を worklog に書いてから着手する。
- 持ち主を移すと、`method(:x).owner`、`instance_methods(false)`、`super` の行き先、`ancestors` の中の探す順が変わる。本の corelib.re:137, 349 の出力と合うことを確かめる。
- 性能: よく呼ばれるメソッド（数の比較など）の探し方が変わるので、下の「性能」を見ること。

## Y4. B（文言を rc2 に合わせる）

README の「B — wording only」の表の 11 原因と、X2 の `eval-error-line`。
- 大きさが中のもの（`conv-float-msg`、`conv-int-msg`）は `convert.rs` と数のメソッドの文言。rc2 の文言の作り（`mrb_convert_type` などのどの関数がどの文言を出すか）を調べて写す。手で 1 つずつ写さない。
- 本が引用する文言（corelib.re:442、extension.re:474、`corelib_error_messages.rb`）と一致することを確かめる。

## Y5. C の記録（コードは変えない）

次の 7 つを README "Known deviations" に 1 行ずつ、`tests/custom/` に 1 つずつ記録する（期待値は SabiRuby の答え、rc2 の答えはコメントと `.rc.out`。`fill-empty-range` は CRuby の答え）。
- `argc-splat`（splat で受けたときだけ rc2 が数える前に変換する）
- `frozen-set`
- `set-uninit`
- `int-div-float`
- `eval-scope-custom-only`（今はテストの頭書きにしか書いていない）
- `kw-dropped`: README の push の項の理由（「mruby は他ではキーワードの Hash を位置引数として渡す」）は事実と合わないので書き直す。rc2 はキーワードを受けないネイティブでは Hash を黙って捨てる。SabiRuby は push を含め、捨てずに位置引数として渡す。push の項と 1 つにまとめてよい。
- `bigint-float`（`(2**70).quo(3)` の件も一緒に）
- `fill-empty-range`

本で触れているものがあれば、book_mruby3 の notes にも書く（本体が行うので、該当を報告するだけでよい）。

## Y6. 全体の確かめ

- Y0 の道具で全体を回し直し、`diff` で前回と比べる。A と B の原因が 0 になり、C は記録済みの印（k）に移ることを確かめる（`compare.py` の `known()` に規則を足す）。新しく出た原因があれば、表に 1 行足して報告する（新しい直しのラウンドにしない）。
- rc2 の mrbtest（3 ビルド）を下げない。`tools/custom.sh`、`tools/edge_compare.sh` で再計測する。
- `docs/verification/rc2-sweep/` の表と README を回し直した結果で更新し、日付と基準のコミットを書く。

## 性能

`OP_ADD`（Y2 の `string-add-redefine`）、`OP_SETMCNST`（Y1）、持ち主の移動（Y3）は速い道に触る。触った段で `tools/bench_ab.sh --core 2` を回す（A/B と A/A、同時に 1 つだけ）。A/A の幅を超えて遅くなったら止めて報告する。命令ループの腕に文言の組み立てを入れない（冷たい関数に出す）。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。根拠のない数を置かない。道具の数は Y0 で `docs/numbers.md` に載せる。
- 公開 API が変わるなら CHANGELOG に書く。利用者に見える変更は Unreleased に 1 行ずつ。backlog の該当行を消す。
- 段ごとにコミットしてよい。worklog は `../worklog/2026-10-18-rc2-sweep-fixes.md`。main にコミットし、push しない。
- 段の到達点を満たしたら止まって報告する。表に無い新しい差を見つけても、直さずに表に 1 行足す。
