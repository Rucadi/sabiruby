# 4.1.0-rc2 との残りの差を直す計画

作成 2026-10-16。`follow-master-plan.md` の作業で見つかった、edge-diff-fix より前から SabiRuby が 4.1.0-rc2 と違っていた点（`../worklog/2026-10-16-follow-master.md`「戻した後も残る、前からの rc2 との差」、114〜122 行と 329 行目以降）。著者の判断（2026-10-16）: 「残りの差も対応して」。

正解は 4.1.0-rc2 の本家 `mruby`（Docker `kishima/mruby:4.1.0-rc2`）。master（`891d1d9d` 以降）でだけ変わった挙動には合わせない（著者の決定、2026-10-16）。

| # | 差 | rc2 の挙動 |
|---|---|---|
| L1 | `m.class_eval { K2 }` の定数を、与えたクラス（`m`）から引く | ブロックの Proc の字句スコープから引く（`MRB_PROC_TARGET_CLASS`）。見つからなければ NameError |
| L2 | `private` の下の `attr_*` を private にする。別のクラスに向けた `other.attr_accessor` も | どちらも public |
| L3 | `method_defined?` が 2 つ目の引数を受け、private のメソッドに false を返す | 引数 1 個（2 個は ArgumentError）、private のメソッドにも true |
| L4 | `module_function` の下の `attr_reader` を private にする | しない |
| L5 | 特異クラスの名前の NameError の文言（`'#<Class:0x…>'`）、`Array#[]=` の負の添字の IndexError の文言（CRuby の文言） | `#<Class:DefInSdef>`、`index -5 out of array` |

## 進め方

- 1 件ずつ rc2 の本家で挙動を確かめ、本家のどのコードに当たるか（`src/vm.c`、`src/class.c`、`src/array.c` など）を worklog に書いてから直す。
- 直したものは `tests/custom/` に、期待値を rc2 の本家の出力にしたテストを足す。既存の custom テストが前の答えを期待していたら、rc2 の答えに直す。
- 直した結果、他の rc2 の挙動とぶつかるものが出たら、止めて報告する。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。
- rc2 の mrbtest（3 ビルド）を下げない。上がったら baseline を上げる。
- 通常時の性能を落とさない。L1（定数の探索）は `bm_ao_render` などに響きうるので、`tools/bench_ab.sh` で変更前後を交互に 2 巡以上、A/A の線と比べる。今この PC で他のベンチは走っていないので、計画の中で測ってよい。
- 公開 API が変わるなら CHANGELOG の Unreleased に。挙動が変わるので CHANGELOG の Unreleased に 1 行ずつ。

## 最後に

- `tools/edge_compare.sh` で再計測し、`docs/verification/edge-compare/` を更新する（公開ページは SabiRuby だけのレポート）。
- `README.md` "Known deviations"、`docs/verification/mrbtest-notes.md` に関係する記述があれば合わせる。
- worklog `../worklog/2026-10-16-rc2-leftovers.md`。
- main にコミット。push しない（本体がレビューしてから push する）。
