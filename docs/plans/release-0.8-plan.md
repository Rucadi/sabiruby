# 0.8.0 の計画: rc2 との差を片付けた状態で出す

作成 2026-10-19。著者「いったん状況を整理して、sabiruby のリリースしようか？」（2026-10-19）。

## 状況（2026-10-19、main `0857e51`）

- 前のリリース: 0.7.0（2026-09-27、tag `v0.7.0`、`4d3036a`）。それから 89 コミット、うち 31 が未 push（`9375b09` から `0857e51`、rc2-sweep と rc2-sweep-fixes）。CI は push 済みの `rc2-leftovers10` まで通っている。
- 0.7.0 の後に入ったもの（CHANGELOG の Unreleased、約 530 行）:
  - 互換の基準を 4.1.0-rc2（stable）に固定（2026-10-16）。master に合わせていた分を戻した。
  - rc2-leftovers 7〜10: 本の記述と食い違う挙動の直し、引数の宣言（`object::Aspec`）。
  - rc2-sweep（2026-10-18）: 977,521 呼び出しと 500 本余りのプログラムを rc2 と比べた洗い出し。
  - rc2-sweep-fixes Y0〜Y8: A（rc2 に合わせる）と B（文言）を 0 に、C（本家の不具合、SabiRuby は正しい方）を README と `tests/custom/` に記録。危ない差 3 つ（panic など）を直した。メソッドの持ち主を rc2 に合わせた。性能は Y7（`OP_ADDILV`）と Y8a（可視性のキャッシュ）、Y8d で、11e595c に対して全体 -7.7%。
- 確かめの数（`0857e51`）:
  - mrbtest（3 ビルド）: utf8 2346、bytes 2269、no-regexp 1981。
  - custom: 85 / 83 / 77。
  - cargo test: 276。
  - edge_compare: 30/17/1、858/33/34/2。
- 公開 API の変更（CHANGELOG の `API:` の行）は、壊すものを含む。
  - `HashData` のメソッドが減った。
  - `InstanceKind` の列挙が増えた（`#[non_exhaustive]` ではない）。
  - `HeapObject`・`CallInfo` のフィールドが増えた。
  - 増えたもの: `Host::compile_eval` と `EvalError`、`object::Aspec`。

  0.x の semver では minor を上げる。

## バージョン（原則で決めた。0.6・0.7 と同じ考え方）

| crate | 今 | 次 | 理由 |
|---|---|---|---|
| `sabiruby` | 0.7.0 | **0.8.0** | 公開 API を壊す変更がある |
| `sabiruby-compiler` | 0.4.0 | **0.5.0** | `Host::compile_eval` の実装（行を返す）、eval の生成エラーを stderr に出さない。`sabiruby` 0.8 に依存する |
| `sabiruby-serde` | 0.3.0 | **0.4.0** | 中身は変わらないが `sabiruby` 0.8 に依存する（0.7 の型とは別物になる） |
| `sabiruby-cli` | 0.7.0 | **0.8.0** | release の名前（VM のバージョン）に合わせる |
| `sabiruby-macros` | 0.1.1 | 0.1.1 のまま | 変わったのはテスト 1 行だけ。依存は syn と quote だけ |

## 段階

| 段階 | 中身 | 担当 |
|---|---|---|
| R1 | 整理。CHANGELOG の Unreleased を `## 0.8.0 — <日付>` に切り、頭に 10 行ほどの要約（何が変わったか、壊す API、互換の基準）を置く。バージョンの番号と crate 間の依存の番号。README・docs の数（mrbtest、custom、edge、本家比）を今の値に。`cargo publish --dry-run` を 4 つ（依存の順）。 | 実装担当 |
| R2 | 下流の確かめ（ファイルは書かない）。rubevy と sabiruby-playground をスクラッチパッドに写し、`[patch.crates-io]`／path で 0.8 を指して建てる。rubevy は `cargo check --all-features` とテスト、`web/build.sh garden` と Playwright（pageerror 0、`?selftest`）。壊れる所があれば一覧にする（直すのは各 repo の次の計画）。 | 実装担当 |
| R3 | ベンチ。`v0.7.0` と候補を `tools/bench_ab.sh` で比べ、`docs/verification/bench.md` に 0.8.0 の節を足す。0.7 と同じく PC が空いた時間に取る。 | 実装担当（**本体が合図する**） |
| R4 | 公開: main を push、CI が通るのを見る、`cargo publish`（`sabiruby` → `sabiruby-compiler` → `sabiruby-serde` → `sabiruby-cli`）、tag `v0.8.0`。 | **著者の確認の後に**（外に出す操作） |
| R5 | 下流のバージョン上げ（rubevy 0.3 など）。各 repo の計画として別に立てる。 | 著者の判断の後 |

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。
- 数を上げるのは R1 だけ。挙動は変えない（R1〜R3 でコードの直しが要ると分かったら、止めて報告する）。
- R2 は他の repo にファイルを作らない（スクラッチパッドの写しで確かめる）。
- push・publish・tag は R4 で、著者の確認の後。

## 状況

| 段階 | 状況 |
|---|---|
| R1 | 未着手 |
| R2 | 未着手 |
| R3 | 未着手 |
| R4 | 未着手 |
| R5 | 未着手 |
