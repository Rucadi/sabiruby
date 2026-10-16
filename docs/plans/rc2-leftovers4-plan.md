# Hash の「空いたエントリ」と、rc2 との小さな差

作成 2026-10-16。著者の判断（2026-10-16）: Hash は案 A（「おすすめで直して」）、「小さな差もポリシーに従って対処して」。方針は `../design/compatibility.md`。

## P1. Hash の探索中に `eql?` が一致したエントリを消す場合（案 A）

- rc2 は空いたスロットを読む（未定義の動作）。mruby master が不具合として直した（`a54353ecf`、走査は `08a0432d1`、`eb7693857`）。方針の「本家が後で不具合として直した」に当たるので、**master の答え（RuntimeError `hash modified`）を採る**。
- 走査（`assoc`、`rassoc`、`==`、`eql?`、`inspect`、`__except`、`rehash` など、master の 2 つのコミットが直したもの）も、master のコミットが直した範囲に合わせる。master の他の挙動の変更まで持ち込まない（その 3 つのコミットの範囲だけ）。
- 記録は方針の 3 か所: README "Known deviations"（rc2 の `src/hash.c` の該当箇所、master のコミット、CRuby の答え、こちらの答え）、`tests/custom/`（期待値は master の出力、rc2 と CRuby の答えをコメントに）、本で触れる価値があれば `book_mruby3/docs/notes/values-and-collections.md`。
- `tests/edge-spec/notes.tsv` の `hash.rb` の理由を合わせる（vacated の 3 つが通るなら、その旨が分かるように消すか書き直す）。
- 性能: `ds_hash`、`app_json_hash` で A/B。

## P2. 小さな差（`../backlog.md`「rc2 との小さな差」）

1 件ずつ方針で判断する。本家に合わせるのが基本。明らかな不具合なら正しい方を採って記録。迷うものは止めて報告。

- `slice!` の凍結 Array の文言（rc2 は `: [1]` を付けない）
- `s["z"] = …` の文言（rc2 は `string not matched` だけ）
- `Array#delete_at` と 2 引数の `MatchData#[]` が `mrb_as_int` でなく `expect_int`
- `array.rs` の使われない `slice!`（`ext_array` が上書きする）: 使われていないことを確かめて消す
- 片付けの途中で同じ種類（文言・整数変換）の差がほかに見つかったら、同じ扱いでまとめてよい。数が多ければ一覧にして backlog に残す。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。
- rc2 の mrbtest（3 ビルド）を下げない。
- CHANGELOG の Unreleased に 1 行ずつ。backlog の「rc2 との小さな差」の行は、直した分を消す。
- `tools/edge_compare.sh` で再計測。
- worklog `../worklog/2026-10-16-rc2-leftovers4.md`。main にコミット、push しない。book_mruby3 に足したらその repo にもコミット（push しない）。
