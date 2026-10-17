# rc2-leftovers8 で見つかった 5 件

作成 2026-10-17。差は `../worklog/2026-10-17-rc2-leftovers8.md`「残ったこと」。著者の判断（2026-10-17、選択肢で「方針で分けて直す」）。方針は `../design/compatibility.md`。正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`、ソースは `/home/kishima/book/ref/mruby` の 4.1.0-rc2。**このクローンの HEAD を動かさない**。ほかの repo にファイルを作らない・消さない）。

## V1. rc2 に合わせる

- **rc2 に無いメソッドを消す**: `Float#to_int`、`Integer#pred`、Integer の `nan?`、`Integer#__num_to_a` など。Float・Integer・Numeric・Comparable のメソッド一覧を rc2 と突き合わせ、rc2 に無いものを洗い出す（同じ種類が String・Array・Hash などにもあれば、表にして worklog に。消すのは数の 4 クラスまで、他は backlog）。消す前に sabiruby の tests・bench・mrblib 相当、rubevy、rubevy_games、playground の例を grep し、使っていれば止めて報告。
- **ネイティブの `arity`**: rc2 は `MRB_ARGS_*` の宣言から arity を出す（`mrb_proc_arity` の C 関数の扱いを確かめる）。SabiRuby のネイティブは全部 -1。宣言を持たせるのが大きな仕事なら、rc2 の答えの規則と SabiRuby に要る変更を見積もって止めて報告。
- **引数の数の誤りの文言**: rc2 の `OP_ENTER`（`argnum_error`）の文言に合わせる（省略できる引数があっても m1+m2 を言う）。全部の Ruby メソッドに効くので、文言の組み立ては cold な関数に置き、`bench_ab` で全体を A/B（A/A も）。
- **捕まえない例外の CLI の表示**: rc2 の `mruby` コマンドの出力（`trace (most recent call last):` と各行）を確かめ、SabiRuby の CLI を合わせる。VM の crate ではなく CLI の変更で済むならそこで。行番号やファイル名の形も rc2 で確かめる。

## V2. 記録だけ

- **`Integer#infinite?`**: rc2 は Integer で false（`num_infinite_p`）、Float で nil。本家の中で食い違う（方針の「mruby の中で一貫しない」）。今の nil のまま、方針の 3 か所に記録（README Known deviations、`tests/custom/`、本の notes は触れる価値があれば。ただし book_mruby3 には書かず、書く案を報告に入れる）。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。新しい数を埋め込まない。
- rc2 の mrbtest（3 ビルド）を下げない。`tests/custom/` に rc2 の出力を期待値にしたテスト。
- CHANGELOG の Unreleased に 1 行ずつ（スクリプトの答えが変わるもの、ホストから見える変化はそう書く）。
- `tools/edge_compare.sh` で再計測。
- worklog `../worklog/2026-10-17-rc2-leftovers9.md`。main にコミット、push しない。
