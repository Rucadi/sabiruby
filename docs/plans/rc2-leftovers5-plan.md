# rc2-ivar で見つかった残りの差

作成 2026-10-17。著者の判断（2026-10-17）: 「残っている差もポリシーに従って対処して」。差は `../worklog/2026-10-17-rc2-ivar.md`「気づいた点」。方針は `../design/compatibility.md`（本家に合わせる。明らかな不具合だけ正しい方を採って 3 か所に記録。迷えば止めて報告）。正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`）。

## R1. 多重代入の右辺の `to_ary`

- `a, b, c = MyAry.new`（`to_ary` を定義した非 Array）が rc2 は `[MyAry のオブジェクト, nil, nil]`、SabiRuby は b と c にもオブジェクトが入る。本の `book_mruby3/contents/overview.re`「暗黙の型変換（`to_int`、`to_str`、`to_ary`）をしない」に関わる。
- rc2 の `OP_AREF`／`OP_APOST`／codegen の多重代入（`gen_massignment`、`mrb_vm_exec` の該当命令）を読み、SabiRuby の codegen と VM の答えを rc2 に合わせる。block の引数の展開（`|a, b|` に `to_ary` を持つ値を渡す）、`*splat` との組み合わせ、ネスト（`a, (b, c) = …`）も rc2 で確かめ、同じ仕組みで違っていれば合わせる。
- 本の記述と rc2・直した後の SabiRuby が合うことを確かめ、worklog に書く。本文は直さない（違えば報告）。
- 性能: 多重代入は速い道に関わる。`bench_ab` で全体と、多重代入を使うベンチを A/B。

## R2. `remove_instance_variable` の名前の検査

- rc2 の `mrb_iv_name_sym_check` を写す。`instance_variable_get`／`set`／`defined?` が同じ検査を通っているかも確かめ、漏れがあれば同じく合わせる。

## R3. Regexp の `instance_variables` の順

- rc2 は `[:@flags, :@source, …]`。`ext_regexp.rs` の書く順を rc2 に合わせる（rc2 の mrblib／C で書く順を確かめる）。MatchData など同じ作りの型も確かめる。

## R4. `Kernel#instance_variable_names`（rc2 に無いメソッド）

- 本家に無いメソッドなので、方針どおり本家に合わせて無くすのが基本。sabiruby の `tests/`・`bench/`・mrblib 相当、rubevy、rubevy_games、playground で使っている所を grep し、使っていなければ消す。使っていれば消さずに止めて報告する（隠れた iv を返さないようにする案と一緒に）。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。新しい数を埋め込まない。
- rc2 の mrbtest（3 ビルド）を下げない。`tests/custom/` に rc2 の出力を期待値にしたテストを足す。
- CHANGELOG の Unreleased に 1 行ずつ（R1・R4 はスクリプトの答えが変わる変更として）。
- `tools/edge_compare.sh` で再計測。
- 範囲の外で見つけた差は直さず、worklog の「気づいた点」と backlog に。
- worklog `../worklog/2026-10-17-rc2-leftovers5.md`。main にコミット、push しない。本の notes に足す価値があれば book_mruby3 にもコミット（push しない）。
