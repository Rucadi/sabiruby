# インスタンス変数を持てない値を rc2 に合わせる

作成 2026-10-17。著者の判断（2026-10-17）: 「RC2にあわせる。ドキュメントにのこして」。見つかった経緯は `../worklog/2026-10-16-rc2-leftovers4.md` P2、`../backlog.md`「インスタンス変数を持てない値」。

## Q1. `obj_iv_p` の外の値にインスタンス変数を置けないようにする

- 正解は 4.1.0-rc2（Docker `kishima/mruby:4.1.0-rc2`）。`src/variable.c` の `obj_iv_p`（OBJECT・CLASS・MODULE・SCLASS・HASH・CDATA・EXCEPTION だけが真）と、それを使う所すべて（`mrb_iv_set` の ArgumentError "cannot set instance variable"、`mrb_iv_get` が nil を返す、`mrb_iv_defined`、`mrb_iv_remove`、`instance_variables`、`instance_variable_get/set`、`SETIV`/`GETIV` 命令、`mrb_obj_iv_inspect`、`dup`/`clone` の iv のコピー、`freeze` との順序など）を読み、SabiRuby の対応する所を同じ答えにする。即値（Integer・Float・Symbol・true/false/nil）も rc2 の答え（FrozenError でなく ArgumentError など、実際に rc2 で確かめた答え）にする。
- SabiRuby の型と rc2 の型の対応（Struct、Data、Exception、Range、Proc、大きな Integer、Set、Time、Random、ホストの値など）は 1 つずつ rc2 で確かめて、表を worklog に残す。SabiRuby で heap の型でも rc2 で iv を持てないものは持てなくする。
- **先に確かめること（壊れるものが無いか）**: rubevy（`/home/kishima/book/kishima/rubevy`）、rubevy_games（`/home/kishima/book/kishima/rubevy_games`）、sabiruby-playground の例、sabiruby の `tests/`・`bench/`・`src/mrblib/` 相当の Ruby、移植した gem の Ruby 部分。持てなくなる値に `@x` を置いている所、`instance_variable_set` を使う所を grep し、該当があれば直す前に止めて報告する（ホストの Rust 側から値に iv を書く API があればそれも）。
- 速さ: `SETIV`/`GETIV` の速い道に型の判定を足すなら、命令ループの腕に文言の組み立てを入れない（L1・M2 の教訓）。`bench_ab` で `vmo_ivar` 系と全体を A/B。

## Q2. `Integer(2**70)` の TypeError（rc2-leftovers4 で見つかった不具合）

- `src/builtins/ext_kernel.rs` の `Integer()` が `Value::Int` だけを Integer と見るので、大きな整数が "can't convert Integer into Integer" になる。rc2 は数をそのまま返す。rc2 の `mrb_convert_to_integer` を読み、大きな整数と、同じ扱いの漏れ（`Float()`、`Rational`/`Complex` があればそれも）を rc2 の答えにする。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。新しい数を埋め込まない。
- rc2 の mrbtest（3 ビルド）を下げない。`tests/custom/` に rc2 の出力を期待値にしたテストを足す（Q1 は型ごと、Q2 は 1 つ）。
- 本との対応: 本の `book_mruby3/contents/overview.re` の「Array と String のサブクラスにインスタンス変数を持てない」の記述と、直した後の SabiRuby の答えが合うことを確かめ、worklog に書く。本文の記述と rc2 の答えが違えば本文は直さず報告する。
- CHANGELOG の Unreleased に 1 行ずつ（Q1 は互換性が変わる変更として、スクリプトが例外になりうることを書く）。backlog の「インスタンス変数を持てない値」の行は消す。
- `tools/edge_compare.sh` で再計測（数が変われば公開ページも変わる）。
- worklog `../worklog/2026-10-17-rc2-ivar.md`。main にコミット、push しない。
