# 2026-10-15 mruby/edge との互換テスト比較

計画: `docs/plans/edge-compare-plan.md`（E1〜E4）。mruby/edge が公開している互換テストのレポート
（https://mrubyedge.github.io/mrubyedge/mruby-spec/）と同じことを SabiRuby で行い、なるべく同じ条件で
並べて HTML にする。この作業は測って比べるだけで、VM の crate（`src/`）には一切手を入れていない。

## E1. SabiRuby で mruby/edge の helpers を走らせる

### mruby/edge の仕組みを読む

`../ref/mrubyedge` の作業ツリーは手元の master（`origin/master` より 84 コミット古い）なので触らず、
`git worktree add --detach <scratch>/edge/mrubyedge origin/master`（`d15d933`）で別に取り出して読んだ。
読んだのは `mrubyedge/tests/mruby_spec/` の `README.md`、`helpers.rb`、`mod.rs`、`events.rs`、
`pattern.rs` と `scripts/mruby_spec_report.py`。

- `mod.rs` の `run` は、自分自身（cargo test のバイナリ）を `MRUBYEDGE_SPEC_CHILD` 付きで子プロセスとして
  起こし、30 秒を過ぎたら kill する。`execute` は `VM::empty()` に `install`（`_spec_event`、`t_print`、
  `_str_match?` の 3 つだけ）をして、`helpers.rb` をコンパイルして走らせ、続けてテストファイルに
  `"\n$spec_completed = true\n"` を足したソースをコンパイルして走らせる。その後 `$spec_completed` を見て
  （無ければ「VM が途中で返った」）、`report` を呼び、真ならば成功。
- 各段の前後に `["phase", 名前, ラベル]` の行を、`events.rs` がイベントファイル（環境変数
  `MRUBYEDGE_SPEC_EVENTS`）に追記する。`helpers.rb` の `assert` は `_spec_event('start'|'pass'|...)` を
  呼んで、同じファイルに assert ごとの記録を書く。
- `mruby_spec_report.py` の `summarize` は、このイベント列と子プロセスの終了コード、タイムアウトの有無、
  ログの文字列から、ファイルの状態（passed / failed / compile_error / runtime_error / panic / aborted /
  timeout / premature_return / incomplete）と assert ごとの結果を決める。

本家の `test/assert.rb` は `t_print` と `_str_match?` だけをネイティブに頼り、C の fixture
（`mrbgems/mruby-test/driver.c` など）は別に入れる。mruby/edge の `install` はその fixture を一つも入れて
いない。README にも「C の fixture（`__env_*`、GC の内部、`mrb_vformat`）が要るテストはエラーになり、
成功の代わりの stub は置かない」とある。

### 決めたこと

- **ネイティブは mruby/edge と同じ 3 つだけにする。** SabiRuby の `sabiruby::mrbtest::install` には
  本家のテスト用 C fixture の移植（`TestNotImplement`、`Fiber.yield_by_c_func`、`__env_svar?` など）が
  揃っているが、これを入れると mruby/edge と条件が変わる（`env.rb` や `kernel.rb` の一部が SabiRuby
  だけ通る）。参考列（本家 `assert.rb`）の方は今の `mrbtest` の数え方なので、そちらには入る。
- **`_str_match?` の中身は、本家 `driver.c` の `str_match_p` の移植（`sabiruby::mrbtest::glob_match`）
  を使う。** mruby/edge の `pattern.rs` は同じ規則を書き直したもの（`*`、`?`、`[...]`、`{a,b}`、`\`）。
  どちらが「正しい」かは本家の方なので、既にある移植を呼ぶ。数え方に差が出る可能性はあるが、
  48 ファイルの `assert_match` で両者の答えが違うものは無かった（下の「確かめたこと」）。
- **`t_print` はその場で標準出力に書く。** SabiRuby の `puts` などは `Vm` の中のバッファに溜まる
  （`Vm::take_output`）。タイムアウトで kill されたときにログが空にならないよう、`t_print` の前に
  バッファを吐き出してから書く（順番も保たれる）。
- **`_spec_event` の文字列でも整数でも nil でもない引数**は、mruby/edge では Rust の `Debug` 表記
  （`format!("{:?}", arg.value)`）になる。SabiRuby には同じものが無いので `inspect` にした。
  `summarize` はこれを詳細の文字列として並べるだけなので、状態や数には関わらない。
- **ネイティブは CLI に置く**（`cli/src/edge_spec.rs`、サブコマンド `sabiruby edge-spec`）。
  `NativeFn` は関数ポインタで捕捉ができないので、イベントファイルのパスは `OnceLock` に入れた。
  VM の crate は変えていない。
- **VM の作り方は `sabiruby` コマンド（`run`）と同じ**: `Vm::new`、`load_mrblib`（gem の mrblib も
  今の mrbtest と同じもの）、`eval` 用の `sabiruby_compiler::Compiler`、時計と `sleep`。
  mruby/edge の `VM::empty()` は eval を持たないが、これはそれぞれの「通常の使い方」に揃える、という
  計画の方針どおり。
- **コンパイルは本家 `mrbc`（891d1d9d）を Docker で、`-g` 付きで。** `-g` は `tools/mrbtest.sh` と同じ
  理由（本家のテストドライバはソースからコンパイルするので、行番号とローカル変数名がある）。
  テストファイルには mruby/edge と同じバイト列 `"\n$spec_completed = true\n"` を足したものを
  `target/edge-compare/src/` に書いてからコンパイルする（`tests/edge-spec/` の写しは変えない）。
  `mrbc` が失敗したファイルは `.mrb` が無いので、子プロセスは phase `compile` の後で止まり、
  `summarize` は `compile_error` と読む（mruby/edge の「compile の phase の後に panic」と同じ流れ）。
- **子プロセスの起こし方と状態の判定は、mruby/edge の `mruby_spec_report.py` の `summarize`・
  `read_events`・`STATUSES`・`NOTE` をそのまま写した**（`tools/edge-compare/run.py`、BSD-3 の表記付き）。
  同じ規則で分類するため、書き直さなかった。

### Docker イメージ

既存の `kishima/mruby:4.1.0-rc2` の作り方は sabiruby の中には無かった。`docker history --no-trunc` を
見ると、本の側の `ref/mruby_containers/rake_nobison_install_bookworm.Dockerfile` と層が一致する
（`ruby:3.2.2-bookworm`、`git clone`、`git checkout $MRUBY_VER`、`rake && rake install`、2 段目に
`COPY --from=build /usr/local/`）。これを `tools/edge-compare/mruby-image.Dockerfile` に写し、2 段目で
`ARG MRUBY_VER` を宣言し直す 1 点だけ変えた（元のままだと 2 段目では引数が見えず、イメージに残る
ソースが既定ブランチの先頭になる。ビルドされる `mrbc` 自体は 1 段目のもので正しい）。
`tools/edge-compare/build-image.sh` で `kishima/mruby:891d1d9d` を作った。
`git rev-parse HEAD` は `891d1d9dc90f1578eedd1003d645250d94eb07e9`。

`mruby --version` は `mruby 4.0.0 (2026-04-20)` と答える（rc2 のイメージは `mruby 4.1.0RC2
(2026-09-11)`）。891d1d9d の `include/mruby/version.h` が 4.0.0 / 2026-04-20 のままだからで、
master のバージョン表記が 4.1.0-rc2 のリリース側と別になっているだけ。`version.rb` のテストは
`RUBY_VERSION` などの形しか見ないので、この差で結果は変わらない。
`mrbc --version` は `mrbc RITE0400 (unknown unknown unknown) Parser: Prism-1.9.0`。

### テストの取り込み

`tests/edge-spec/upstream/` に、`git -C ../ref/mruby archive 891d1d9d test/t test/assert.rb LICENSE` で
取り出したものを置いた。mruby/edge の `SHA256SUMS` を写して `sha256sum -c` で照合し、48 ファイル全部
OK。`assert.rb` と `LICENSE` も mruby/edge の写しとバイト単位で同じ（`cmp`）。`helpers.rb` は
`d15d933` のものを `cp` し、両方の SHA-256 が同じ（`b8243062…`）であることを確かめた。
`tools/edge_compare.sh` は毎回、走らせる前に `SHA256SUMS` を照合する。

### 確かめたこと

mruby/edge の `mod.rs` にある driver の自己テスト（`assertion_driver`）と同じ Ruby を SabiRuby で
走らせ、同じ数になることを確かめた。

```
passing : OK
failing : 
  assertion failed:  expected 1, actual 2
FAIL
exception : ERROR: unexpected
...
[nil, nil, 1, 3, 1, 1, 1, 1]
OK: 1, FAIL: 3, ERROR: 1, WARN: 1, SKIP: 1, STRESS: 1
```

mruby/edge の期待値は ok 1、ko 3、kill 1、skip 1、stress 1、warning 1、`$wrong_exception_result` と
`$missing_exception_result` が nil で、すべて一致した。終了コードは 1（report が偽）。

`_str_match?` の差も確かめた。`helpers.rb` の後に `_str_match?` を包んで引数と答えを
`_spec_event('glob', ...)` に書く数行を足したもの（スクラッチの写し。`tests/edge-spec/` は変えていない）で
48 ファイルを走らせると、呼ばれたのは 18 回。`pattern.rs` の `match_bytes` を Python に 1 対 1 で写した
もので同じ 18 組を判定し、SabiRuby の答えと 18 回とも一致した。

### 結果（E1 の時点）

`tools/edge_compare.sh`（48 ファイル）: passed 29、failed 18、runtime_error 1。
assert の合計は pass 856、fail 35、error 34、warn 0、skip 2、stress 17、interrupted 0。
参考列（同じ 48 ファイルを本家 `assert.rb` と C fixture 付きで）: total 976、ok 882、ko 45、
crash 30、skip 2。詳しくは E3 の HTML と下の一覧。

## E2. mruby/edge を手元で走らせる

`scratch/edge/mrubyedge`（`origin/master` の worktree、`d15d9338fdb896ef306d887d641c7470b33f9eab`、
2026-10-14）で、README のとおり `python3 scripts/mruby_spec_report.py --format json` を走らせた。
既定の features（`wasi`、`mrubyedge-debug`）、`--features` の指定なし、上限 30 秒。

最初は手元でそのまま走らせて、ビルドで止まった。

```
thread 'main' panicked at .../bindgen-0.73.2/lib.rs:618:27:
Unable to find libclang: "couldn't find any valid shared libraries matching: ['libclang.so', ...
error: cargo build failed; no compatibility report generated
```

mruby/edge のコンパイラ（`mruby-compiler2-sys`）は bindgen を使い、bindgen は libclang を要る。この PC には
libclang が無い（`2026-09-16-rbs-study.md` で `ruby-rbs` が同じところで止まっている）。システムに入れる
のはやめ、`rust:1.97.0-bookworm`（手元の `rustc --version` と同じバージョン）に `libclang-dev clang
python3 git` を足しただけのコンテナで走らせることにした（`tools/edge-compare/mrubyedge-runner.Dockerfile`、
`tools/edge-compare/run-mrubyedge.sh`）。同じ PC の Docker（WSL2 の上）で、子プロセスの起こし方も上限も
スクリプトそのまま。worktree の `.git` はリポジトリ本体を指すファイルなので、本体の `.git` を同じ絶対
パスに読み取り専用で mount し、`GIT_OPTIONAL_LOCKS=0` で `git status` が書かないようにした。
作業ツリーは `target/`（ビルドとレポート）以外変わらない。PC の側では、この間に別の担当の Ruby の
テスト（4 プロセス）が動いていた。時間は数えておらず、上限 30 秒に近いファイルも無いので、結果には
関わらないと見ている。

結果（2026-10-15T09:47:53Z）: passed 9、failed 23、panic 6、aborted 3、runtime_error 4、compile_error 3。
assert は pass 119、fail 36、error 234、warn 1、skip 0、stress 15、interrupted 7。

**公開ページとの差**: 公開ページ（`docs/mruby-spec/index.html`、commit `b31d3346`、生成
2026-10-14T05:47:04Z）のファイルごとの表（状態と 7 つの数）を読み取って突き合わせると、48 ファイル
とも同じだった（差 0）。`d15d933` は `b31d3346` にレポートを足しただけのコミットなので、VM は同じもの。

JSON は `docs/verification/edge-compare/mrubyedge.json` に写した。その中の `log` と `events` のパスは
`target/edge-compare/mrubyedge/` からの相対で、docs には置いていない。
