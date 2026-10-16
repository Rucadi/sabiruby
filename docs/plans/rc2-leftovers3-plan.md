# `Array#[]` の Float の端と、方針に沿った 2 つの差の記録

作成 2026-10-16。著者の判断（2026-10-16）: 「Array#[] も直して」、互換の方針（`../design/compatibility.md`）を書いた。

## N1. `Array#[]` の Float の端と、数でない添字の文言

- rc2 は `[1,2,3,4,5][1.0..2.0]` が `[2,3]`。SabiRuby は TypeError。数でない添字の文言も違う（`../worklog/2026-10-16-rc2-leftovers2.md` 気づいた点 1）。
- 読む側の引数の処理（`index_args`）は String と共有している。直す前に、`String#[]`（と `slice`、`slice!` など共有している他のメソッド）で rc2 が同じ入力に何を答えるかを確かめ、Array と String のどちらも rc2 の答えになる形で直す。String 側で rc2 と違う答えになる場合は、共有を分けるか、止めて報告する。
- rc2-leftovers2 の M3・M4 で作った `numeric::as_int` を使えるなら使う。
- 性能: `ds_array`、`ds_string`、`vmo_index` を中心に A/B（命令ループの腕に文言の組み立てを入れない。L1・M2 の教訓）。

## N2. 既に決めていた 2 つの差を、方針に沿って記録する

`../design/compatibility.md` の「明らかな不具合」に当たるものとして、すでにこちらの挙動を採っているもの・採るべきものを、方針の 3 か所に記録する。

- **`a[0, nil] = 1`**（rc2-leftovers2 で写さなかった rc2 の `aget_index` の癖）: README "Known deviations" に、rc2 の `src/array.c` の該当箇所、CRuby の答え、こちらの答え。`tests/custom/` に期待値（CRuby の答え）のテストと、rc2 の答えのコメント。
- **Hash の探索中に `eql?` が一致したエントリを消す場合**（follow-master の M1 で rc2 の見える答えを写したもの。rc2 は空いたスロットを読む＝未定義の動作）: master が直したコミットと CRuby の答えを確かめ、方針どおり不具合として「正しい方」を採るなら直す。どちらが正しいか（master の答えと CRuby の答えが違う場合など）で迷うなら、直さずに案を書いて止めて報告する。直したら、`hash.rb` の vacated の 3 つの assert（891d1d9d のテスト）の notes.tsv の理由も合わせる。
- 本で触れる価値がありそうなら、`/home/kishima/book/book_mruby3/docs/notes/` に 1 行ずつ（どのファイルに書くかは notes の README の目次に従う）。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。
- rc2 の mrbtest（3 ビルド）を下げない。
- CHANGELOG の Unreleased に 1 行ずつ。backlog の「rc2 との残りの差」の行を、直した分だけ減らす。
- `tools/edge_compare.sh` で再計測。
- worklog `../worklog/2026-10-16-rc2-leftovers3.md`。main にコミット、push しない。book_mruby3 に足したらその repo にもコミット（push しない）。
