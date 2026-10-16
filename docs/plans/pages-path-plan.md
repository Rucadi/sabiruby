# 公開ページのパスを `mruby-compatibility/` にする

作成 2026-10-17。著者の指示（2026-10-17）: 「mruby互換性の表の公開がedge-compareという名前ではなくて、mruby互換性という名前のパスにしてほしい」。

## G1. 公開パスの変更

- GitHub Pages で `https://sabiruby.github.io/sabiruby/edge-compare/` に出している SabiRuby の互換レポート（`docs/verification/edge-compare/index.html` と JSON 2 つ）を、`https://sabiruby.github.io/sabiruby/mruby-compatibility/` で出す。
  - 名前の判断: 指示は「mruby互換性」。公開ページは英語なので `mruby-compatibility`（省略しない）。
- 旧パス `edge-compare/` には、新しいパスへ移す転送ページ（`<meta http-equiv="refresh">` と、新しい URL へのリンク 1 行）を置く。すでに外にリンクが出ているかもしれないため。中身は英語で事実だけ（"This page moved to …"）。転送ページは repo に置き、workflow でコピーする（`docs/compare/` の下など、置き場所は docs/README の目次に合うように決めて記録）。
- 変えるもの: `.github/workflows/pages.yml`（コピー先、先頭のコメント）、`docs/compare/index.html` のリンク、`tools/edge-compare/report.py` の docstring の URL、`docs/verification/edge-compare/README.md` の URL、`docs/README.md` の該当行。ほかに公開 URL を書いている所があれば grep で拾う（worklog と過去の plans は歴史の記録なので書き換えない）。
- 変えないもの: repo の中のディレクトリ名（`docs/verification/edge-compare/`、`tools/edge-compare/`、`tools/edge_compare.sh`）。これは「mruby/edge の比較を作る道具」の名前で、公開パスとは別。`report.py` が生成するページ自身に自分の URL を書いているなら、それは新しい URL に。
- `report.py` を触ったら `python3 tools/edge-compare/report.py` で生成し直し、生成物の差がパスの文字列だけであることを確かめる（数は変わらない）。

## 守ること

- worklog は作らず、この計画書の末尾に結果を 3〜5 行で書く。main にコミット、push しない（push は著者の確認の後に本体が行う）。
- Pages の確認は push 後に本体が行う。手元では `pages.yml` の `run:` 部分を一時ディレクトリで実行して、`_site/` の構成（`mruby-compatibility/index.html`、JSON 2 つ、`edge-compare/index.html` の転送、`compare/mrubyedge.html`、`index.html`）を確かめ、報告に `find _site` の結果を貼る。

## 結果（2026-10-17）

- `pages.yml` は報告と JSON 2 つを `_site/mruby-compatibility/` へコピーする。旧パスの転送ページは `docs/compare/edge-compare/index.html` に置いた（`docs/compare/` はサイトの素材の置き場所で、サイトの中のパスと同じ名前にすると対応が見てわかる。`pages.yml` の `paths` の `docs/compare/**` にも入る）。`<meta http-equiv="refresh">`・`rel=canonical`・リンク 1 行。
- リンクを直したのは `docs/compare/index.html`、`docs/compare/mrubyedge.html`（grep で拾った `../edge-compare/`）、`report.py` の docstring、`docs/verification/edge-compare/README.md`、`docs/README.md`。生成されるページ自身は自分の URL を書いていないので、`report.py` で生成し直した `index.html`・`compare.html` は差が無かった。
- 手元で `run:` をスクラッチの一時ディレクトリで実行し、`_site/` に `index.html`、`compare/mrubyedge.html`、`edge-compare/index.html`（転送）、`mruby-compatibility/{index.html,sabiruby.json,sabiruby-fixtures.json}` ができることを確かめた。Pages 上の確認は push 後。
