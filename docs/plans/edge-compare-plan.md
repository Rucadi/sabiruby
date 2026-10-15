# mruby/edge との互換テスト比較の計画

作成 2026-10-15。著者の依頼（2026-10-15）: mruby/edge の公開レポート（https://mrubyedge.github.io/mrubyedge/mruby-spec/）と同じことを SabiRuby でも行い、「なるべく同じ条件」で比べて、他の人にも見せられる HTML にする。

## 前提（本体が調べたこと）

- mruby/edge の比較は `mrubyedge/tests/mruby_spec/`（`../ref/mrubyedge`、`origin/master` `d15d933`、2026-10-14）。
  - テストは mruby upstream `891d1d9dc90f1578eedd1003d645250d94eb07e9`（2026-10-12）の `test/t/*.rb` 48 ファイルを、バイト単位でそのまま写したもの（`upstream/SHA256SUMS`）。
  - 本家の `assert.rb` は使わず、自前の `helpers.rb` で数える。数えるのは「到達した `assert` ブロック」（入れ子を含む）。結果は pass / fail / error / warn / skip / stress / interrupted。ファイルの状態は passed / failed / compile_error / runtime_error / panic / aborted / timeout / premature_return / incomplete。
  - 1 ファイルを 1 子プロセス・新しい VM で走らせ、上限 30 秒（`--timeout` の既定値）。ファイル末尾に印を足して、途中で VM が返ったのを見分ける。
  - コンパイラは同梱の `mruby_compiler2_sys`（本家の mrbc ではない）。
  - 報告は `scripts/mruby_spec_report.py`（Python 標準ライブラリのみ、`--format json|html|md`）。ライセンスは BSD-3-Clause（Uchio Kondo）。
- SabiRuby の今の mrbtest（`tools/mrbtest.sh`）は 4.1.0-rc2 の 47 ファイルと gem のテストで、本家 `assert.rb` を使い、本家 mrbc（Docker `kishima/mruby:4.1.0-rc2`）でコンパイルする。
- rc2（`c17ffcc`）から `891d1d9d` までに 519 コミットある。`include/mruby/ops.h` と `include/mruby/dump.h` には差が無い（命令表とバイトコード形式は同じ）。`test/t` は 4774 行増え、`refinement.rb`（600 行）が新しく入った。

## 条件の揃え方（本体が決めたこと）

| 項目 | 揃え方 | 理由 |
|---|---|---|
| テスト | 両方とも `891d1d9d` の `test/t` 48 ファイル。`SHA256SUMS` で mruby/edge のものと同一であることを確かめる | 同じ問題を解かせる |
| 数え方 | **mruby/edge の `helpers.rb` をそのまま SabiRuby で使う**。SabiRuby 側は helpers が呼ぶネイティブ（`t_print`、glob、イベント出力など mruby/edge の `mod.rs` の `install` にあるもの）を CLI 側に用意する | 数え方の違いを無くす。本家 `assert.rb` での SabiRuby の数（今の mrbtest の数え方）は参考として別の列に出す |
| 実行 | 1 ファイル 1 プロセス、新しい VM、上限は mruby/edge と同じ 30 秒、末尾の印で途中終了を見分ける。状態の分類も mruby/edge と同じ名前にする | 状態の比較ができるように |
| コンパイラ | SabiRuby は `891d1d9d` から作った本家 mrbc（Docker イメージを新しく作る。既存の `kishima/mruby:4.1.0-rc2` は変えない）。mruby/edge は自前のコンパイラのまま | それぞれの実装の通常の使い方。compile_error はコンパイラの差として表に注記する |
| mruby/edge の結果 | 公開ページを写すのではなく、**同じ PC・同じ日に `origin/master` を手元で走らせた JSON**（`--format json`、既定の features）を使う。公開ページのコミット（`b31d3346`）と数が合うかも 1 行で確かめる | 同じ日・同じ PC |
| SabiRuby の features | 既定（`utf8` と `regexp`）。gem の mrblib も今の mrbtest と同じものを読み込む | 通常の使い方。mruby/edge も既定の features |

決めたことの補足:
- 既存の `tools/mrbtest.sh` と baseline（rc2）は変えない。新しい比較は別の道具にする。rc2 の baseline を `891d1d9d` へ上げるかどうかは別の話で、著者に聞く（この計画の範囲外）。
- 新しいテストで SabiRuby が落ちても、**この計画では直さない**。数えて、理由が分かるものは注記するだけ。refinement が無いなどの大きな差は、最後に一覧で報告する。

## 段階

### E1. SabiRuby で mruby/edge の helpers を走らせる

- `891d1d9d` の mrbc の Docker イメージを作る（`docker info` が通らなければ止めて報告。Docker Desktop は起動しない）。
- `891d1d9d` の `test/t` を取り込む（置き場所は `tests/mrbtest/` の作法に合わせて別ディレクトリ。MIT の表記）。`SHA256SUMS` を mruby/edge のものと照合する。
- mruby/edge の `helpers.rb` を取り込む（BSD-3 の表記と出どころのコミットを README に）。改変しない。必要なネイティブは CLI 側（`sabiruby-cli`）に置き、VM の crate には入れない。
- 1 ファイルずつ子プロセスで走らせ、mruby/edge と同じ形の JSON（`schema_version: 1` の `files`、assert ごとの記録、ログ）を出す道具を作る（`tools/edge_compare.sh` など。名前は既存の作法に合わせる）。上限 30 秒は引数で変えられるようにし、既定値の出どころ（mruby/edge の既定値）を `docs/numbers.md` に書く。
- 参考列: 同じ 48 ファイルを本家 `assert.rb` で走らせた SabiRuby の数（今の `mrbtest` サブコマンドの数え方）。

### E2. mruby/edge を手元で走らせる

- `../ref/mrubyedge` の `origin/master` を作業用の場所（worktree か scratch）に取り出し、`python3 scripts/mruby_spec_report.py --format json` を走らせる。`../ref/mrubyedge` の作業ツリーそのものは変えない。
- コミット、実行日、features を記録する。公開ページ（`b31d3346`）との差を 1 行で。

### E3. 比較の HTML

- 1 枚の自己完結した HTML（外部の script を読まない。CSS はインライン）を Python 標準ライブラリで生成する。
  - 冒頭: 条件の一覧（テストのコミット、両者のコミット、コンパイラ、数え方、実行日、PC）と、差を読むときの注意（compile_error はコンパイラ、C の fixture（`env.c` など）が要るテストは両方とも動かない、など）。
  - 全体の数: 両者の pass / fail / error / … の合計、全部通ったファイル数。
  - ファイルごとの表: 両者の状態と数を横に並べる。
  - ファイルを開くと assert ごとに両者の結果を並べる（assert の説明文と順番で突き合わせる。片方にしか無い行も出す）。失敗の詳細も。
  - 参考列: SabiRuby の本家 `assert.rb` での数。
  - ライト／ダーク両方で読めること。スマートフォンの幅で横にはみ出さないこと（表は横スクロールの枠に入れてよい）。
- 生成物: JSON 2 つと HTML を `docs/verification/edge-compare/` に置く（ログは大きければ `target/` に置き、docs には置かない）。`docs/verification/README` の目次に追加。
- 公開は本体が行う（Artifact）。

### E4. 記録

- `docs/worklog/2026-10-15-edge-compare.md`: 手順、決めたこと、数、SabiRuby が新しいテストで落ちたものの一覧（理由が分かるものは理由、分からないものは「未調査」）。
- `docs/README.md` の目次、`docs/numbers.md`（30 秒）。

## 守ること

- VM の crate に `unsafe` を入れない。VM の crate に手を入れる必要が出たら止めて報告（この計画は測るだけ）。
- 既存の mrbtest、baseline、`cargo test` を壊さない。
- mruby/edge のファイル（`helpers.rb`、テスト）は改変しない。改変が要るなら止めて報告。
- コミットは段階ごと。push はしない（本体がレビューしてから）。
