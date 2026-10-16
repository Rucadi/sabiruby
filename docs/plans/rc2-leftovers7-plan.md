# 残りの小さな差を方針で片付ける

作成 2026-10-17。著者「小さな差もポリシーに従ってチェックして」でチェックし（表は同じ場所の `rc2-leftovers7-check.md`。rc2・master 891d1d9d・SabiRuby 28cbf01・CRuby 3.2.6 で実際に動かした）、著者「おすすめで対処して」（2026-10-17）。方針は `../design/compatibility.md`。正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`、ソースは `/home/kishima/book/ref/mruby` の 4.1.0-rc2。**このクローンの HEAD を動かさない**。別のコミットを読むときは `git show <rev>:path`）。

## T1. rc2 に合わせる（チェックの表の A）

- `sort` の比較の失敗の文言、`values_at` に Integer 以外（Array と Struct）、引数 3 つの `raise`、`private [:a]`／`private "a"`、`private :a` などの返り値（4 メソッド）、`private_class_method`／`public_class_method` を消す、`expect_str` のラベル（`Regexp.union`・`join`・`include?`・`index`・`[]=`）、`Float#floor/ceil/truncate(n)`（桁を読む）、`Array#pop(n)`、`Integer#<<` に Float、`Integer.__ensure` に 2**70、`shuffle(random:)` の長さ 1 以下。
- **整数の引数の変換**（66 メソッド）: `expect_int` の誤りの腕を rc2 の `mrb_as_int` の答えに（文言だけの 41）、値まで違う 25 は個別に（約 12 か所）。`expect_int` は呼び出しが多い。命令ループや速い道の腕に文言の組み立てを入れず、cold な関数に出す（L1・M2・rc2-leftovers4 の教訓）。
- `tests/custom/` に rc2 の出力を期待値にしたテスト。

## T2. rc2 が明らかな不具合（表の B）: 方針の 3 か所に記録

- `a[NaN]`・`a[Inf]`・`a[1e30]`、`rand(NaN)`・`rand(Inf)`: rc2 は C の未定義の動作。master `e2148c1b1` の答え（RangeError）を採る。コードを直す。
- `Integer#to_s(nil／1.5)`（rc2 は型を見ずに値のビット列を読む）、`eval("__method__")` と eval の中の `super`（master `b0b1de54e` が直した）: 今の SabiRuby の答えのまま、記録だけ。
- 記録: README "Known deviations"（rc2 の関数、master のコミット、CRuby の答え）、`tests/custom/`（期待値はこちらの答え、rc2 の答えをコメントか `.rc.out` に）、本で触れる価値があれば `book_mruby3/docs/notes/`。

## T3. 著者の判断（2026-10-17、おすすめどおり）

- **`Struct#hash`: 合わせない。** hash の値は SabiRuby 独自（`1.hash` から rc2 と違う）。`Array#hash` を rc2 の式にすると Array をキーにした Hash が遅くなりうる。README に「hash の値と、異なる型の間で hash が等しいかどうかは SabiRuby 独自」と 1 文。
- **凍結した特異クラス: 今のまま（`def` も `extend` も拒む）にして記録。** rc2 は同じクラスで `def` を通し `extend` を拒む（本家の中で食い違う。`def` が通るのは ccdf75c1c の文言の分岐の副作用）。方針の 3 か所に記録。
- **`instance_exec`／`class_exec` の `__method__`: nil のままにして記録。** rc2 と master は `:instance_exec`、`instance_eval` は nil（食い違う。master は eval の分だけ直した）。方針の 3 か所に記録。

## T4. 挙動の差でないもの（表の D）

- 使われないネイティブ（mrblib に上書きされる 37、同名のネイティブに上書きされる約 45）を、使われないことを確かめて消す。regexp なしのビルドで使う `string.rs` の 12 個は残す（`check_no_std.sh` の両方で確かめる）。`tools/coverage.sh` で `coverage.md` を作り直す。
- `FromRuby for Vec<T>`（`src/convert.rs`）を `plain_ary_vals` にする（Struct・Data を Array として受け取らない）。ホスト API の変化として CHANGELOG に 1 行。rubevy・games に該当なし（チェックで grep 済み、念のためもう一度）。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。新しい数を埋め込まない。
- rc2 の mrbtest（3 ビルド）を下げない。
- CHANGELOG の Unreleased に 1 行ずつ（スクリプトの答えが変わるものはそう書く）。backlog の該当行を消し、T3 の判断を残す。
- 性能: `bench_ab` で全体と、変えたメソッドを使うベンチを A/B。A/A も取る。
- `tools/edge_compare.sh` で再計測。
- worklog `../worklog/2026-10-17-rc2-leftovers7.md`。main にコミット、push しない。book_mruby3 に足したらその repo にもコミット（push しない）。
- 量が多いので、T1〜T4 ごとにコミットを分けてよい。
