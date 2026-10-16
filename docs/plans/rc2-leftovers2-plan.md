# 4.1.0-rc2 との残りの差、その 2

作成 2026-10-16。`rc2-leftovers-plan.md`（L1〜L5）の作業で見つかった、範囲外の差（`../worklog/2026-10-16-rc2-leftovers.md` 気づいた点、`../backlog.md`「rc2 との残りの差」）。著者の判断（2026-10-16）: 「悪影響あまりなさそうで直しやすそうなら直してみてほしい」。

正解は 4.1.0-rc2 の本家 `mruby`（Docker `kishima/mruby:4.1.0-rc2`）。

| # | 差 | rc2 |
|---|---|---|
| M1 | クラス `Foo` の `Foo::String`／`Foo.const_get(:String)` が Object の `String` を返す | NameError（クラスを受け手にしたときは Object を探さない） |
| M2 | クラスでない受け手（`1::X` など）の TypeError の文言 | rc2 の文言 |
| M3 | `Array#[]=` で Range の端が Float のとき TypeError | 通る（整数にする） |
| M4 | `a[nil] = 1` の文言 | rc2 の文言 |

## 直すかどうかの線

1 件ずつ、直す前に次を見る。どれかに当たるなら直さず、理由を worklog と報告に書く（backlog の行は残す）。
- **悪影響**: 今まで通っていたスクリプトが例外になる変更か。M1 はそれに当たりうる（`SomeClass::String` のように Object の定数をクラス越しに書いているコード）。sabiruby の `tests/`、`bench/`、rubevy（`/home/kishima/book/kishima/rubevy`）、rubevy_games（`/home/kishima/book/kishima/rubevy_games`）、playground の例の `.rb` を grep して、当たる書き方が無いか確かめる。本家の mrblib と gem の Ruby 部分（`src/mrblib/` の元）も。当たるものがあれば直さず報告。
- **直しやすさ**: 本家の対応するコードが小さく、写して済むか。定数の探索の速い道（L1 で作った `const_lookup` の形）を崩す必要があるなら、性能を測ってから決める（A/A の線を越えたら直さない）。
- **4.1 の挙動か**: master でだけ変わった挙動に合わせない。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。
- rc2 の mrbtest（3 ビルド）を下げない。
- 直したものは `tests/custom/` に rc2 の出力を期待値にしたテストを足す。CHANGELOG の Unreleased に 1 行ずつ。
- 直したら `tools/edge_compare.sh` で再計測（数が変われば公開ページも変わる）。
- backlog の「rc2 との残りの差」の行を、直したものの分だけ減らす（全部直したら行を消す）。
- worklog `../worklog/2026-10-16-rc2-leftovers2.md`。main にコミット、push しない。
