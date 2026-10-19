# 2026-10-19 比較ページを 0.8.0 の時点に

著者の指示（2026-10-19「比較ページ更新して」、ベンチも可）で、mruby/edge との比較ページ
`docs/compare/mrubyedge.html` を SabiRuby 0.8.0（tag `v0.8.0` = `7f74cd1`）の時点に更新した。
前回の作り方は `docs/plans/edge-compare-plan.md` と `docs/worklog/2026-10-15-edge-compare.md`。
コードは変えていない。測ったのはすべて同じ PC（WSL2、`vmstat` の idle 98〜99%、ほかの担当の
cargo・sabiruby・mruby の実行は無し）。

## mruby/edge 側: 9 コミット進んでいた

`ref/mrubyedge` で `git fetch` すると、`origin/master` は `e30a5e0`（2026-10-16 16:07 +0900）で、
ページの `d15d933`（2026-10-14）から 9 コミット（`c7dd9ae` から数えると 93）。マージ 3、
`github-actions[bot]` のレポート更新 2、残りの 4 つは次のとおり（コミットと差分で確かめた）。

- `8d314d4`（Dahrkael）: `fnv` を常に使う依存にした。`mruby-hash-fnv` は「既存のビルド行列との互換のため」
  何もしない feature として残り、`RHashMap` などは常に fnv。`vm.rs` に `fnv_hash` が足された
  （コミットメッセージによれば、これから入れるディスパッチと ivar のキャッシュのため。今は未使用）。
- `f95a388`: バージョンを 2.1.2 に（`mrubyedge`、`mrubyedge-cli`）。
- `cd3fdaa`: 互換レポートを別のリポジトリ `mrubyedge/mruby-spec` から公開するようにした。
  新しい URL は https://mrubyedge.github.io/mruby-spec/ 。旧 URL（`/mrubyedge/mruby-spec/`）は
  `meta refresh` で転送している（curl で確かめた）。新しいページは `e30a5e0` で作られていた。
- `ee4781e`: レポートの HTML に進み具合の棒、ファイル名の検索、結果の絞り込み、コミットへのリンク。

HEAD（`c7dd9ae`）は動かさず、`git worktree add --detach <scratch>/edge08/mrubyedge origin/master`
で取り出し、`tools/edge-compare/run-mrubyedge.sh` で前回と同じ手順のレポートを取った
（`mruby_spec_report.py` は +102 行変わっているが、JSON の形 `schema_version: 1` は同じ）。
結果は 48 ファイルの状態も assert の数も `d15d933` と 1 つも変わらない:
passed 9、failed 23、panic 6、aborted 3、runtime_error 4、compile_error 3。assert は pass 119、
fail 36、error 234、warn 1、stress 15、interrupted 7。ファイルごとに状態と数を突き合わせて差は 0。
`docs/verification/edge-compare/mrubyedge.json` を差し替え、`compare.html`（非公開）を作り直した。

ページに載せる edge 側の数も数え直した（`git grep` を両方のコミットで）。`mrubyedge/src` の `.rs` は
13,398 → 13,399 行、`use std::` は 44 → 45（`fnv_hash` の中の `use std::hash::Hasher`）、unsafe は
同じ 10 行（ページは `Sync`／`Send` の 2 行を 1 と数えて 9 か所）。テストは `d15d933..e30a5e0` で
`tests/mruby_spec/README.md` しか変わっていないので、ページの「64 本」はそのまま（ただし、今
`tests/*.rs` を数えると 53、再帰で 68 で、64 の数え方は再現できなかった。下の「気づいた点」）。

## SabiRuby 側: 数は 0.7.0 の後の記録と同じ

`tools/edge_compare.sh` を main `f06a23a`（`7f74cd1` の後は docs だけ。`git diff 7f74cd1 HEAD -- src
cli compiler macros serde Cargo.*` は空）で、作業ツリーが clean な状態（`dirty: false`）で走らせた。
`target/release/sabiruby` はこの走行で建て直され（mtime が `generated_at` と同じ）、
`sabiruby 0.8.0 (...)` と答える。

| | 前回（`4fc10d1`、2026-10-18） | 今回（`f06a23a`、0.8.0） |
|---|---|---|
| ファイル | passed 30、failed 17、runtime_error 1 | 同じ |
| assert | pass 858、fail 33、error 34、skip 2、stress 17 | 同じ |
| fixtures 付き | passed 30、failed 18 / pass 883、fail 43、error 31 | 同じ |

ファイルごとの状態と数も 48 ファイルすべて同じ。JSON は commit、日付、`vm_version` だけが替わった。

## wasm のサイズ: 前回の条件で測れた

ページは「同じ条件で測ったのは 2026-09-12 の 1 回だけ」と書き、条件は `docs/design/playground.md`
にある: VM だけの `wasm32-unknown-unknown` の cdylib（RITE を 1 つ読んで走らせる関数を 1 つ export）、
release、`lto = true`、`codegen-units = 1`、`panic = "abort"`、`strip = true`、`wasm-opt -Oz`
（binaryen 132）、gzip -9。プローブのソースは残っておらず、2 行のスニペットだけがあるので、同じ形で
書き直した（スクラッチの `edge08/probe/`。export は `run(ptr, len)` で、入力を受ける境界に
`core::slice::from_raw_parts` の unsafe が 1 つある。どちらのプローブも同じ形）。

最初の `wasm-opt -Oz` は検証エラーで止まった（`memory.copy operations require bulk memory`）。今の
rustc は wasm32 で bulk memory などを既定で使うので、playground の `tools/build.sh` と同じ
`--enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals`
を足した（exception-handling は Rust の出力に無いので外した）。

**条件が本当に同じかを、旧バージョンを建て直して確かめた。** rustc 1.97.0（2026-09-12 とは別の
toolchain）で、crates.io の `mrubyedge` 1.1.12 と、`v0.2.0` の worktree の SabiRuby を同じプローブで:

| | 記録（2026-09-12） | 今日の建て直し | 差 |
|---|---:|---:|---:|
| mruby/edge 1.1.12 | 562,560（gz 185,343） | 562,928（gz 185,421） | +0.07% |
| SabiRuby 0.2.0 | 783,729（gz 281,054） | 785,072（gz 280,760） | +0.17% |

0.2% 以内で合ったので、この手順は 2026-09-12 と同じ条件と見てよいと判断した。今のバージョン:

| | bytes（`wasm-opt -Oz` 後） | gzip -9 | `wasm-opt` 前 |
|---|---:|---:|---:|
| mruby/edge 2.1.2（`e30a5e0`、既定の features `wasi`、`mrubyedge-debug`） | 579,166 | 192,914 | 697,347 |
| SabiRuby 0.8.0（既定の features: `std`、`utf8`、`regexp`） | 2,001,572 | 707,958 | 2,283,763 |
| SabiRuby 0.8.0、`--no-default-features` | 1,226,351 | 441,957 | 1,419,974 |

**2026-09-13 の数は再現しなかった。** ページは「gem を入れた後（2026-09-13）は 1,261,940 バイト、
`--no-default-features` で 937,307」と書いていた。記録したコミット `d9fe60c` を worktree に出して同じ
プローブで建てると 1,971,127 バイト（gz 685,267）で、合わない。`d9fe60c` の差分を読むと、その前の節は
`opt-level = "z"` で測っていたので、`opt-level = "z"` でも建ててみた（全部のプローブを
`CARGO_PROFILE_RELEASE_OPT_LEVEL=z` で）:

| opt-level "z" | bytes | gzip -9 |
|---|---:|---:|
| mruby/edge 1.1.12 | 348,831 | 134,223 |
| SabiRuby 0.2.0 | 545,287 | 212,883 |
| SabiRuby `d9fe60c` / `--no-default-features` | 1,397,181 / 1,071,503 | 515,499 / 416,403 |
| mruby/edge 2.1.2 | 388,311 | 144,219 |
| SabiRuby 0.8.0 / `--no-default-features` | 1,420,224 / 815,549 | 533,045 / 323,097 |

`opt-level = "z"` では 2026-09-12 の 2 行が合わず（562,560 に対して 348,831）、`d9fe60c` の数も
1,261,940 にならない。2026-09-13 の数がどの条件で取られたかは分からないので、ページではこの 2 つの数を
使わず、再現しなかったことを 1 文で書いた。ページの表は 2026-09-12 の条件（opt-level 既定）の数だけにした。

## 性能: 27 本のうち 12 本で、2 つの VM を同じベンチで測った

**この節の数はページに載せない**（著者の判断、2026-10-19）。測るためのコマンド（mruby/edge の VM を
呼ぶ側）をこちらで組んでおり、相手に不利な条件が混じっているおそれがあるため。ページの性能の行は前と同じく
「2 つの VM どうしの比較は未測定」とし、数と測り方はこの worklog にだけ残す。

mruby/edge の VM に `.mrb` を渡すだけの小さなコマンドを作った（スクラッチの `edge08/edgerun/`。
`rite::load`、`VM::open`、`run`。サイズのプローブと同じ呼び方。`mrubyedge` は既定の features で、
`mrubyedge-math` などの別の crate は入れていない。release は SabiRuby の release と同じ
`codegen-units = 1`、lto なし）。CLI の `mrbedge run` を使わなかったのは、CLI が依存する
`mruby-compiler2-sys` が libclang を要り、この PC には無い（前回はコンテナで回した）ため。VM の実行だけを
比べるなら、コンパイラの無いこのコマンドで足りる。

`bench/*.mrb`（本家 mrbc 4.1.0-rc2 でコンパイル済み、SabiRuby のベンチと同じファイル）27 本を、両方で
1 回ずつ走らせ、標準出力を突き合わせた。同じ出力になったのが 12 本。残り 15 本の理由（mruby/edge の
エラー出力から）:

- `NameError`: `Math`（app_robot）、`ARGV`（bm_ao_render）、`Fiber`（call_fiber）、`GC`（gc_churn と
  vmo_* の 5 本）
- `NoMethodError`: `putc`（bm_mandel_term）、`print`（bm_so_mandelbrot）、`FalseClass#!`（bm_so_lists）、
  `Hash#key?`（ds_hash）
- vm_optimization_bench: 引数の無い `puts` のところで Rust の panic（`prelude/object.rs:160` の
  `args[0]`、終了コード 101）
- mem_retained: 120 秒の `timeout` で止めた（SabiRuby は 1 秒弱）

12 本を、P コア 2 番に固定（`taskset -c 2`）、1 組 5 回、最良と中央値、2 つの VM の順番を入れ替えて
2 巡（1 巡目は SabiRuby → mruby/edge、2 巡目は逆）で測った。巡の間の差は最大 1.3%
（call_block_yield の mruby/edge）。SabiRuby のバイナリは edge_compare と同じもの（0.8.0）をスクラッチに
写して使った。表は 2 巡のうち良い方の最良（秒）:

| benchmark | SabiRuby | mruby/edge | 比（edge / SabiRuby） |
|---|---:|---:|---:|
| app_json_hash | 1.036 | 2.520 | 2.43 |
| app_tak | 1.109 | 9.870 | 8.90 |
| bm_fib | 5.058 | 45.799 | 9.05 |
| call_args | 0.799 | 8.257 | 10.33 |
| call_block_yield | 0.960 | 8.326 | 8.68 |
| call_kwargs | 0.683 | 5.249 | 7.69 |
| ds_array | 0.764 | 13.577 | 17.77 |
| ds_string | 0.340 | 1.271 | 3.73 |
| loop_if_branch | 0.737 | 7.036 | 9.55 |
| loop_times | 0.810 | 3.523 | 4.35 |
| loop_while_add | 0.712 | 8.217 | 11.55 |
| mem_short_lived | 0.806 | 5.903 | 7.32 |
| **合計** | **13.81** | **119.55** | **8.65**（中央値 8.79） |

時間はプロセスの起動からの壁時計（Python の `perf_counter` で `subprocess.run` を挟む）で、両方とも
同じ測り方。走らせたスクリプトは次のとおり（スクラッチの `edge08/scripts/vmbench.py`）:

```python
for r in range(rounds):
    order = vms if r % 2 == 0 else vms[::-1]
    for b in names:
        for vm, binp in order:
            ts = []
            for _ in range(runs):
                t0 = time.perf_counter()
                subprocess.run(['taskset', '-c', core, binp, f'bench/{b}.mrb'],
                               stdout=subprocess.DEVNULL, check=True)
                ts.append(time.perf_counter() - t0)
```

mruby/edge の目的は小さく動かすこと（README）で、ページの冒頭にも目的が違うと書いてある。ページには数と
条件と、比べなかった 15 本の理由だけを書いた。

## ページの書き換え

- meta: mruby/edge `e30a5e0`・2026-10-16・2.1.2、SabiRuby `v0.8.0`（`7f74cd1`）・2026-10-19・0.8.0。
  見出しの日付を 2026-10-19 に、足元に「更新 2026-10-19」。
- 「前回からの主な変化」: mruby/edge は 93 コミットにし、2026-10-16 の 2 項（公開先の移動とレポートの
  見た目、fnv と 2.1.2）を足した。SabiRuby は「0.8.0 まで」にし、基準を rc2 にして master に合わせた
  変更を戻したこと、rc2 との差の洗い出し（977,521 回の呼び出しと 502 本）と直し、0.7.0 より 8.5%
  短くなったことを足した（CHANGELOG 0.8.0 の要約から）。
- 数: crates.io の 5 crate（0.8.0 / 0.5.0 / 0.8.0 / 0.1.1 / 0.4.0）、`sabiruby-compiler` 0.5.0、本家比
  2.28 倍（中央値 2.70 倍。`docs/verification/bench.md` "0.8.0 candidate" と
  `bench/results/release-0.8/ref-e7a3381.md`）、行数（SabiRuby `src/` 28,936、mruby/edge 13,399）、
  `use std::` 45、gem の行の `mruby-hash-fnv`、mruby/edge のレポートの URL。
- wasm のサイズの行を、上の測定で書き換えた。性能の行は SabiRuby の本家比だけを替え、2 つの VM どうしは
  「未測定」のまま（一度載せたが、著者の判断で外した。上の節）。「未確認・未測定」には別の crate を入れたときの
  サイズを足した。
- 書き方（著者、2026-10-19、本体経由）: SabiRuby は高い速度を求めておらず、マイコンでの実行も想定して
  いない。本家比の速度は事実の数として書き、短所として扱わない。ページの「2.57 倍遅い」「本家比の遅さは
  縮んだ」を「実行時間の比は 2.28 倍」「2.57 倍から 2.28 倍になった」に直した。あわせて mruby/edge の
  例外の行の「前回の課題だった」を「前回の表で挙げた」にした（外のプロジェクトを評価する言い方を避ける）。
- 元のファイル（本の側の `ref/`、git の外）: `ref/mrubyedge-vs-sabiruby-2026-10.html` をページと同じ中身に
  した（Artifact 用に doctype の 4 行と `body { margin: 0; }` を除き、互換レポートへのリンクは絶対 URL）。
  `ref/mrubyedge-vs-sabiruby-2026-10.md` には「2026-10-19 の更新」の節を足し、性能の行の「遅さが縮んだ」を
  数だけの言い方にした。表の各行は 2026-10-15 のまま。VM どうしの性能の数はどちらにも載せていない。

worktree は 3 つ作って（mruby/edge `e30a5e0`、SabiRuby `v0.2.0`、`d9fe60c`）、終わってから消した。
mruby/edge の worktree には `target/` ができていたので `worktree remove --force` で消した。

## 気づいた点

- **mruby/edge のレポートの旧 URL がまだ残っている所**: `tools/edge-compare/report.py`（公開している
  `docs/verification/edge-compare/index.html` のリンク）、`docs/verification/edge-compare/README.md`、
  計画と 2026-10-15 の worklog。旧 URL は転送されるので壊れてはいない。コードの変更になるので今回は
  直していない。属する先: SabiRuby の互換レポートの道具。
- **ページの「自前のテストが 64 本」の数え方が再現できない**: `mrubyedge/tests/*.rs` は 53、`tests/` の
  下を再帰で数えると 68（`d15d933` でも `e30a5e0` でも同じ）。2 つのコミットの間でテストは変わって
  いないので、ページは据え置いた。属する先: 比較ページの次の更新で数え方を決める。
- **`docs/design/playground.md` の 2026-09-13 の wasm の数（1,261,940 / 937,307）は、記録したコミットで
  建て直しても再現しない**（上の節）。playground.md 自体は直していない。属する先: SabiRuby の docs。
- **ほかのセッションの worktree が残っている**: `ref/mrubyedge` の
  `/tmp/claude-1000/-home-kishima-book/4df3b210-.../scratchpad/edge/mrubyedge`（`d15d933`、前回のもの）。
  自分が作ったものではないので消していない。
- 本の素材になりそうなこと: wasm のサイズは toolchain が替わっても 0.2% しか動かなかった一方、
  `opt-level` を "z" にすると 4 割近く小さくなる（mruby/edge 1.1.12 で 562,928 → 348,831）。
  サイズを比べるときは opt-level を条件に書く必要がある。
