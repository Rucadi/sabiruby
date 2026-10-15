# 891d1d9d の本家テストで見つかった差を直す計画

作成 2026-10-15。`edge-compare-plan.md` の比較で、upstream `891d1d9d` の `test/t` に対して SabiRuby が落ちた assert が 69 個あった（`../worklog/2026-10-15-edge-compare.md`「SabiRuby が落ちた assert」）。著者の判断（2026-10-15）:
- 「結果の前に、見つかった本家 mruby との差分は直して再度テストしたい」。
- 比較ページの文章は「mruby/edge と同じような文章で淡々と」。

## 直す対象

本家の挙動の正解は **`891d1d9d` の本家 `mruby`**（Docker `kishima/mruby:891d1d9d`）。新しいテストはこのコミットの挙動を書いたものなので、これに合わせる。rc2 と 891d1d9d で挙動が違う場合は 891d1d9d に合わせ、違ったことを worklog に残す。

| # | 差 | ファイル（件数） |
|---|---|---|
| D1 | 演算子の再定義が届かない（`Integer#+` など算術・比較の速い道）。`basicobject.rb` の `==` も同じ種類か確かめる | integer 3、float 2、symbol 1、basicobject 2 |
| D2 | `$!` が rescue 修飾子と ensure の中で nil。引数なしの `raise` で、rescue 中の例外を投げ直す | exception 3 |
| D3 | `defined?` の定数パス（`A::B`）とレシーバ付きのメソッド。15 個以上の位置引数とキーワード引数 | syntax 8 ＋ rc2 から変わった `defined? (A::B)` |
| D4 | メソッドを置く先と可視性: レシーバ付きのメソッドの中の `def`/`alias`/`undef`、ブロックで書いたクラス本体の `def`、`attr_*`・`define_method`・`class << self` の中の `private`、ブロックで渡されたクラスの中の定数と cvar、`module_function`、`method_defined?` の inherit false、`respond_to?` の可視性 | methods 4、class 1、module 10、kernel 3 |
| D5 | `eql?` が Hash を変える場合（新しい 3 つと、rc2 からある 1 つ） | hash 4 |
| D6 | その他: `Integer#%`（テストが変わった）、`Array#[]` に整数を表さない Float、文字列の式展開が `#to_s` を呼ぶ、`GC.step_ratio=` の範囲 | integer 1、array 1、string 1、gc 1 |

直さないもの（理由を worklog とページの注記に書く）:
- **C の fixture が要るテスト**（`env.rb` の `__env_*`、`TestNotImplement`、`AryShared`、`sysfail`、`vformat`、`nofree_cstr?`、`__iv_set_hidden`、`__gc_add_region_undersized`）。本家の挙動の差ではなく、本家のテスト用 C コード。比較では mruby/edge と同じく入れない（`edge-compare-plan.md` の決定）。
- **著者が以前に「意図した差」として受け入れたもの**（`docs/verification/mrbtest-notes.md`、`README.md` "Known deviations"）: NaN の同一性、`(1..).last`（本家も同じく落ちる）、パターンマッチでキーが対象を書き換える件、`codegen.rb` の `/static/`。ただし D5 の Hash は、新しいテストで本家の意図がはっきりしたので、直せるかを調べる。直すと設計（`Hash` の作り）に大きく響くなら、案を並べて止めて報告する。

## 進め方

- 1 件ずつ、本家 `891d1d9d` の挙動を `docker run kishima/mruby:891d1d9d mruby -e …` で確かめてから直す。本家のどのコード（`src/vm.c`、`src/class.c`、`mrbgems/…`）に対応するかを worklog に。
- 直したものは `tests/custom/` などの既存の作法で、SabiRuby 自身のテストにも足す。
- 段階ごとにコミット（D1〜D6 の単位でよい）。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。
- 既存の mrbtest（rc2）の baseline を下げない。上がったら `--update` で上げてよい。
- **通常時の性能を落とさない**。特に D1 は算術の速い道に触れるので、本家と同じやり方（再定義されたかを、クラスの印などで一度だけ確かめる）を調べてから。`tools/bench_ab.sh` で変更前後を交互に 2 巡以上、A/A のばらつきの幅を線にする（`../verification/bench.md`）。**今は同じ PC で他のベンチは走っていないので、この計画の中で測ってよい**（本体の合図済み）。
- 本家と挙動を合わせるために公開 API が変わるなら CHANGELOG の Unreleased に。

## 最後に: 再テストとページの文章

1. `tools/edge_compare.sh` で再計測し、`docs/verification/edge-compare/` を更新する。rc2 の mrbtest も走らせて docs を更新する。
2. **比較ページの文章を mruby/edge のレポートと同じ調子にする**（著者の指示）。mruby/edge の公開ページ（`docs/mruby-spec/index.html` in mruby/edge）にならい、英語で、メタ情報（コミット、生成日時、features、upstream のリビジョン）、数え方の定義、ファイルの表、assert ごとの表を事実だけで書く。評価や感想、強調（「〜だけ」「大きく」など）、どちらが良いかに読める言い回しは入れない。条件の違い（コンパイラ、C fixture、実行環境）は「Conditions」として事実を列挙する。
3. worklog（`../worklog/2026-10-15-edge-diff-fix.md`）に、直したもの、直さなかったものと理由、性能の数。
