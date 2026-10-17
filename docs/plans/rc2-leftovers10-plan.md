# ネイティブの引数の宣言と、rc2-leftovers9 の残り

作成 2026-10-17。差は `../worklog/2026-10-17-rc2-leftovers9.md`（V1-2 の見積もり、V1-3 の "given"、気づいた点）。著者の判断（2026-10-17、選択肢で）: arity は「宣言を持たせる」、"given" は「今のまま記録」、`define_method` と `Integer.new` の文言は直す。方針は `../design/compatibility.md`。正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`、ソースは `/home/kishima/book/ref/mruby` の 4.1.0-rc2。**このクローンの HEAD を動かさない**。ほかの repo にファイルを作らない・消さない）。

## W1. ネイティブに引数の宣言を持たせる（rc2 と同じ作り）

- rc2: `mrb_define_method` の `MRB_ARGS_*` がメソッドの flags に入り（`src/class.c:1105-1115`）、RProc では `RProc.flags` の 13 ビットに圧縮して持つ（`include/mruby/proc.h:129-179`、`MRB_ARGS_NONE` は `MRB_PROC_NOARG`）。VM は C 関数を呼ぶ直前にこれで引数の数を検査し（`src/vm.c:1609-1615`、`check_argument_count`）、`arity` も同じ値から出す（`src/proc.c:440-449`）。本の notes: `book_mruby3/docs/notes/extension-api.md` の 2026-10-17 の追記。
- SabiRuby: ネイティブの定義（約 930 か所）に rc2 の宣言と同じ値を持たせる。値は rc2 の同名メソッドの `MRB_ARGS_*` から取る（rc2 に無いメソッドは、今の `argc!` の検査と同じ意味の宣言）。呼び出しの道で宣言を見て検査し、宣言と同じ意味の `argc!`（391 か所）を外す。`arity` は宣言から出し、`ext_method.rs` の手書きの表を消す。
- 進め方: 宣言の書き方（定義のマクロ・表の形）を先に決め、`mrb_define_method` の形に近づける。ホストが定義するメソッド（rubevy などが使う公開 API）にも宣言を渡せる形にし、渡さないときは今と同じ（検査なし、arity -1）にして互換を保つ。公開 API が変わるなら CHANGELOG に。
- 宣言の値は rc2 のソースから機械的に集める道具を作り（`tools/` に。rc2 の `MRB_MT_ENTRY` と `mrb_define_*method` の呼び出しを読む）、手で写さない。集めた表と SabiRuby の定義を突き合わせ、合わないもの（rc2 に無い、名前が違う）を worklog に。
- 確かめ方: 両方にあるメソッド 1098 個で `arity` と、引数の数を外したときの誤り（文言と例外のクラス）が rc2 と一致すること。数を測る道具は rc2-leftovers9 のものを使う。
- 性能: 呼び出しの速い道に検査が入る。命令ループの腕に文言の組み立てを入れない。`bench_ab` で全体と `call_*` 系を A/B（A/A も）。A/A の幅を超えて遅くなるなら、止めて報告（宣言の持ち方を変えるか、著者に聞く）。
- 量が多いので、段を分けてコミットしてよい（宣言の仕組み、宣言の値、検査の移し替え、arity）。

## W2. "given" の数は今のまま記録

- rc2 はキーワード引数が絡む 4 つの呼び方で "given 1, expected 1" と自分と矛盾した文言を出す（rc2-leftovers9 の worklog）。本家の中で食い違う（方針の「mruby の中で一貫しない」）。SabiRuby は渡した数を正しく言う今の答えのまま、README の Known deviations と `tests/custom/`（期待値はこちら、rc2 の答えは `.rc.out`）に記録。

## W3. `define_method` の block を strict に

- rc2 は `define_method`／`define_singleton_method` の block を lambda と同じく引数の数を厳密に見る（`define_method(:g){|a,b=1|}` に 3 つで ArgumentError、`arity` -2）。SabiRuby は `[1, 2]` を返し `arity` 1。rc2 の `mod_define_method` の扱い（`MRB_PROC_STRICT` を立てるところ）を確かめて写す。

## W4. `Integer.new` などの文言

- rc2 は "undefined method 'new' for Class"、SabiRuby は "for Integer"。rc2 の文言の作り（受け手が Class のときの `%T`）を確かめ、同じ種類（undef された特異メソッドの文言）をまとめて合わせる。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。新しい数を埋め込まない（宣言の値は rc2 から来る）。
- rc2 の mrbtest（3 ビルド）を下げない。`tests/custom/` に rc2 の出力を期待値にしたテスト。
- CHANGELOG の Unreleased に 1 行ずつ。backlog の該当行を消す。
- `tools/edge_compare.sh` で再計測。
- worklog `../worklog/2026-10-17-rc2-leftovers10.md`。main にコミット、push しない。
