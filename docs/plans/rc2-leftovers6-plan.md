# Struct を Array として扱う Array のメソッドと、片付け

作成 2026-10-17。著者の判断（2026-10-17）: 「１も続けて直してよい、３も片付けて」。差は `../worklog/2026-10-17-rc2-leftovers5.md`「気づいた点」と `../backlog.md`。方針は `../design/compatibility.md`。正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`）。

## S1. Array のメソッドが Struct と Data を Array として扱う

- rc2 は `mrb_array_p`（`MRB_TT_ARRAY` だけ）で判定するので、Struct・Data（`MRB_TT_STRUCT`）は Array ではない。`[1] + s`、`concat`、`replace`、`product` は TypeError、`flatten`、`==`、`<=>`、`Array(s)` も答えが違う（rc2-leftovers5 の worklog）。
- rc5 で足した `Heap::plain_array`（`mrb_array_p` 相当）を使い、Array を引数に取る所・Array かどうかで分ける所を rc2 に合わせる。対象を grep で洗い出し（`ary_vals`／Array の形を借りる判定を使う builtins と ext、Kernel の `Array()`、`Array#==` など）、rc2 で確かめた答えの表を worklog に残す。
- Struct と Data 自身のメソッド（`to_a`、`each`、`==` など、Struct が内部で Array の形を使う所）は壊さない。`tests/custom/` と rc2 mrbtest の struct の数で確かめる。
- ホスト API の `ary_vals` を変えるなら、rubevy・rubevy_games の使い方を grep してから。変えずに済むなら変えない。

## S2. 片付け（rc2-leftovers5 の残り 3・4）

- `builtins/object.rs` の、metaprog に上書きされて使われない `instance_variable_get`／`set`／`defined?`／`instance_variables` を、使われていないことを確かめて消す（rc2-leftovers4 の `slice!` と同じ扱い）。
- `docs/verification/coverage.md` を `tools/coverage.sh` で作り直す。
- backlog の `array.rs` の使われない 15 個も同じ種類なので、同じ確かめ方で消す。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。新しい数を埋め込まない。
- rc2 の mrbtest（3 ビルド）を下げない。`tests/custom/` に rc2 の出力を期待値にしたテストを足す。
- CHANGELOG の Unreleased に 1 行ずつ（S1 はスクリプトの答えが変わる変更として）。backlog の該当行を消す。
- 性能: Array の速い道（`ds_array`、`vmo_index` など）と全体を `bench_ab` で A/B。
- `tools/edge_compare.sh` で再計測。
- worklog `../worklog/2026-10-17-rc2-leftovers6.md`。main にコミット、push しない。
