# rc2-leftovers7 の残り 4 件

作成 2026-10-17。差は `../worklog/2026-10-17-rc2-leftovers7.md`「気づいた点」。著者の判断（2026-10-17、選択肢で「3 件直し sort は記録」）。方針は `../design/compatibility.md`。正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`、ソースは `/home/kishima/book/ref/mruby` の 4.1.0-rc2。**このクローンの HEAD を動かさない**）。

## U1. rc2 に合わせる 3 件

- `Class` で `module_function` を undef する（rc2 の `mrb_init_class` 相当を確かめて写す）。
- `String#hex`／`String#oct` を、rc2-leftovers7 で `String#to_i` に入れた rc2 の走査（`str_to_integer`）に合わせる。接頭辞、`_`、符号、範囲外の答えを rc2 で確かめる。
- `Float#to_i` は引数を取らない（rc2 の `MRB_ARGS_NONE` と同じ誤り）。同じ作りで引数を受け付けてしまう Float／Integer のメソッドがほかにあれば、rc2 で確かめて同じ扱い。
- `tests/custom/` に rc2 の出力を期待値にしたテスト。

## U2. `sort` が block に渡す組の順は合わせない（記録だけ）

- rc2 はヒープソート、SabiRuby はマージソート。比較の順は作りで決まり、Ruby も約束していない。
- 記録の前に、**同じ値の要素の並ぶ順**（`sort_by` や `<=>` が 0 を返す組）が rc2 と違うかを確かめる。違うなら、それも同じ「合わせない差」として書く（どれくらい違うかの例を 1 つ）。
- README の Known deviations の「合わせない差」の文（`instance_variables` の順と同じ段落）に 1 文、backlog の「著者が決めた本家との差」に 1 行。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。新しい数を埋め込まない。
- rc2 の mrbtest（3 ビルド）を下げない。CHANGELOG の Unreleased に 1 行ずつ。backlog の該当行を消す。
- `bench_ab` で全体を A/B（A/A も）。`tools/edge_compare.sh` で再計測。
- worklog `../worklog/2026-10-17-rc2-leftovers8.md`。main にコミット、push しない。
