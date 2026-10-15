# 本家 master に追従する計画（Hash の探索順、性能の戻り、mrblib・テスト・コンパイラ）

作成 2026-10-16。`edge-diff-fix-plan.md` の後。著者の判断（2026-10-16）:
- D5（Hash）: 「C にするのが正攻法であればそうしたい」→ 本家の探索順を写す（案 C）。互換が目的なので本家と同じ作りにするのが正攻法と判断した（本体）。
- 「本家の最新に追従したい」→ 検証の基準を 4.1.0-rc2 から本家 master に移す。
- push は今の作業が終わってから著者が確認する。

**変更（2026-10-16、著者の指摘「master への追従だと 4.1 への対応という話と変わってしまわないか」）**: 本家の 4.1.0-rc2 は `stable` ブランチにあり、master の祖先ではない（分岐点 `4e2bc5c2b`、2026-09-04。その後 master だけに 577 コミット）。master は 4.1 の次の開発なので、**M3・M4 は止めた**。M1（rc2 でも同じ挙動）と M2 は続ける。edge-diff-fix で直したものが rc2 と master のどちらの挙動かを 1 件ずつ確かめ、目標をどうするかは著者が決める。
**著者の決定（2026-10-16）: 4.1 の挙動を守る。** master でだけ変わった挙動に合わせた直しは rc2 の挙動に戻し、比較ページの Conditions にそう書く。

## 基準にするコミット

着手時の本家 master の先頭に固定する（2026-10-16 時点は `158760853`。着手時に `git fetch` して決め、計画書と worklog に書く）。以後、このコミットを「master 基準」と呼ぶ。mruby/edge との比較（`tools/edge_compare.sh`）は mruby/edge が取り込んだ `891d1d9d` のテストのまま変えない（比較の条件だから）。

## 段階

### M1. Hash の探索順を本家に合わせる（D5、案 C）

- master 基準の `src/hash.c`（ar／ht の 2 つの表、探索のときに `eql?` を呼ぶ順番と条件、探索中に表が変わったときの扱い）を読み、SabiRuby の `Hash` の探索を同じ順番・同じ条件にする。`edge-diff-fix` の worklog の D5 の節にある 4 つのテストと、rc2 の `hash.rb` の 1 つ（README "Known deviations" と `mrbtest-notes.md` で「意図した差」としていたもの）が通ることを目標にする。
- Set（`gem_set` の ko、khash の rebuild guard）も同じ種類なら合わせる。
- 0.7.0 で S1（`eql?` の中の例外を握りつぶさない）を入れたときの判断（`release-0.7-plan.md`、worklog）と矛盾しないこと。
- README "Known deviations"、`mrbtest-notes.md`、notes.tsv、CHANGELOG（Unreleased）から、この差の記述を外す。
- 性能: `ds_hash`、`app_json_hash` を中心に A/B。

### M2. 性能の戻りを追う

`edge-diff-fix` で、`bm_ao_render` +5〜7%、`app_robot` +4〜6%、`gc_churn` +3〜4% が A/A の線（±3.2%）を超えた。3 本ともオブジェクトを多く作るベンチなので、本体の見立ては次のとおり。順に確かめる（測ってから直す。当たらなければ「外れ」と worklog に）:
1. `ProcData`（`cref`、`given`）と `EnvData`（`given_class`）にフィールドが増え、`ObjKind`／`HeapObject` が大きくなっていないか（`size_of` を main と比べる）。大きくなっていれば、1 オブジェクトあたりのメモリと割り当て・GC の費用が増え、3 本とも遅くなる。ビットフラグにまとめるなどで元の大きさに戻す。
2. `arith_value`／`op_compare` を通るようになった Float と、オブジェクトの演算子（`Vec#+` のような送り）の道が、前より分岐と関数呼び出しが増えていないか（`app_robot`）。速い道を前と同じ形に戻せるか。
3. ブロック（`each do`、`times do`）で proc を作るときと、定数の探索の cref の歩き方（`bm_ao_render`）。本家の `MRB_CONST_CACHE` 相当（定数ごとのキャッシュ）を入れるかは、1 と 2 の後でまだ線を超えていたら、案として worklog に書き、著者に聞く（入れるのは別の計画）。
- 最後に 27 本の A/B を交互に 2 巡以上。線を超えたものが残れば、原因と案を報告する。

### M3. mrblib と gem のテストを master 基準に

- `src/mrblib/`（core と gem の mrblib）を master 基準の本家 mrbc で作り直す。`tools/mrbtest.sh` の `MRUBY` と `IMG` を master 基準に（Docker イメージ `kishima/mruby:<短いハッシュ>` を `tools/edge-compare/build-image.sh` と同じ作り方で作る）。
- `test/t` と gem の `test/*.rb` を master 基準から取り込み直し、mrbtest を 3 つのビルド（既定、bytes、no-regexp）で走らせる。落ちたものは 1 件ずつ master 基準の本家 `mruby` で確かめ、本家との差なら直す（`edge-diff-fix-plan.md` と同じ進め方。C の fixture は今までどおり移植で）。
- baseline（`tests/mrbtest/baseline*.txt`。`cargo test` が「このファイルは最低これだけ通る」と確かめる表）を master 基準の結果で作り直す。`gem_enum` を下げた件（`Enumerable#count`）は mrblib を上げると戻るはず。
- `docs/verification/mrbtest*.md`、`mrbtest-notes.md`、README の「4.1.0-rc2」の記述を master 基準に。

### M4. 同梱コンパイラを master 基準に

- `sabiruby-compiler`（`compiler/`）が C のままビルドしている本家の `mruby-compiler` と Prism を master 基準に上げる。出力が本家 mrbc とバイト単位で一致することの確認（既存の確かめ方）を通す。
- playground と `eval` もこのコンパイラを使うので、`cargo test` と、playground の wasm のビルドが通ることを確かめる（playground の公開はしない）。
- crate のバージョンと description（「4.1.0-rc2」）の変更は CHANGELOG の Unreleased に書く。リリースはしない。

### M5. 記録と再計測

- `tools/edge_compare.sh` で再計測（テストは `891d1d9d` のまま）し、`docs/verification/edge-compare/` を更新。
- worklog `docs/worklog/2026-10-16-follow-master.md`。`docs/numbers.md`、`docs/README.md` の目次。
- 本の側（`book_mruby3`）は「4.1.0-rc2 で検証」と書いているので、SabiRuby が master 基準になったことを `book_mruby3/docs/notes/` に 1 行（本文は変えない）。

## 守ること

- VM の crate に `unsafe` を入れない。`no_std + alloc`、`Vm: Send + Sync`。
- 段階ごとにコミット。branch は `edge-compare` の上で続ける。push しない。
- ベンチは今この PC で他に走っていないので、計画の中で測ってよい。
- 公開 API が変わるなら CHANGELOG の Unreleased に。
