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

## E3. 比較の HTML

`tools/edge-compare/report.py`（Python 標準ライブラリのみ）が `sabiruby.json`、`mrubyedge.json`、
`tests/edge-spec/notes.tsv` から `docs/verification/edge-compare/index.html`（約 300 KB）を作る。
`tools/edge_compare.sh` の最後でも呼ぶ（mruby/edge の JSON があれば）。

- 中身の順: 合計（両者の 7 つの数、全部通ったファイル数、同じ尺度の帯グラフ）、参考列の合計、
  ファイルの状態の数、条件の表、差を読むときの注意、ファイルごとの表（両者の状態と数、本家
  `assert.rb` での SabiRuby の ok / total、SabiRuby が通らない理由）、assert ごとの突き合わせ。
- **assert の突き合わせ**は、両者の assert の説明文の列を `difflib.SequenceMatcher` で並べる。
  同じ説明文は横に並び、片方しか到達しなかったものは相手側を「not reached」とする。入れ子は
  `parent_id` から深さを出して字下げ。結果が違う行は背景色を変え、「違う行だけ表示」の切り替えを
  付けた（小さな inline script）。
- 自己完結: 外部の script もフォントも読まない（CSS はインライン、フォントはシステムのもの）。
  色はすべて `:root` の変数で、`prefers-color-scheme: dark` と `data-theme` の両方で切り替わる。
  `<title>` は「SabiRuby vs mruby/edge」。
- **英語で書いた。** `docs/verification/` は英語という repo の規則（`docs/README.md`）に合わせ、
  mruby/edge の公開レポートも英語なので、他の人に見せるときにもそのまま読める。

確かめたこと: Playwright（sabiruby-playground の `playwright-core` と `~/.cache/ms-playwright` の
Chromium）で 1200px ライト、400px ダーク、400px ライトを開き、`document.documentElement.scrollWidth`
が `innerWidth` と同じ（1200/1200、400/400、400/400）、pageerror 0。表は横スクロールの枠の中に
入っている。スクリーンショットで、合計と条件と `syntax.rb` の突き合わせを目で見た。

E3 のコミットの後に `tools/edge_compare.sh` を通しで走らせ直し、`sabiruby.json` の `commit` を
そのコミット（`dirty: false`）にした。数は E1 と同じだった。

## E4. 数と、SabiRuby が落ちたもの

### 合計

| | SabiRuby | mruby/edge |
|---|---:|---:|
| 全部通ったファイル | 29 / 48 | 9 / 48 |
| ファイルの状態 | passed 29、failed 18、runtime_error 1 | passed 9、failed 23、panic 6、aborted 3、runtime_error 4、compile_error 3 |
| 到達した assert | 944 | 412 |
| pass | 856 | 119 |
| fail | 35 | 36 |
| error | 34 | 234 |
| warn | 0 | 1 |
| skip | 2 | 0 |
| stress | 17 | 15 |
| interrupted | 0 | 7 |

参考列（同じ 48 ファイル、本家 `assert.rb`、SabiRuby の mrbtest の C fixture 付き、`sabiruby mrbtest`
を 1 ファイルずつ）: total 976、ok 882、ko 45、crash 30、warn 0、skip 2、48 ファイルとも report まで
走った。

mruby/edge の compile_error 3 つ（`string`、`unicode`、`vformat`）は、ログを見るとコンパイラ自体ではなく
`mod.rs` の `mrubyedge::rite::load(&binary).unwrap()` が `InvalidFormat` で panic している。phase が
`compile` のままなので `summarize` は compile_error と読む。HTML の注意書きにそう書いた。

### SabiRuby が落ちた assert（fail 35 と error 34 の内訳）

fail か error の assert は 69 個（同じ assert の中の失敗は 1 つと数える）。rc2 の `test/t` を今の
`sabiruby mrbtest -v` で走らせた結果（説明文で突き合わせ、`[15.2.x]` の ISO 番号は外す）と比べて
分けると、次のとおり。

**rc2 のテストでも落ちていたもの（16）** — 既知。`docs/verification/mrbtest-notes.md` に理由がある。
- `env.rb` の 8 つ全部（`__env_*` の C fixture。rc2 でも crash）
- `array.rb` 「Array shared from an emptied heap array keeps a buffer」（`AryShared` の C fixture）
- `codegen.rb` 「register window of calls (#3783)」
- `float.rb` 「a NaN is the object it is and no other」
- `hash.rb` 「Hash lookup with entries deleted by an eql? callback」
- `range.rb` 「Range#last」（終わりの無い Range で RangeError）
- `syntax.rb` 「pattern matching - a key that moves the subject」
- `sysfail.rb`、`vformat.rb`（C fixture）

**rc2 では通っていて、ここで落ちたもの（8）**
- C fixture が無いため（mruby/edge の条件に合わせて入れていない。SabiRuby の mrbtest には移植がある）:
  `kernel.rb` の `TestNotImplement` を使う 5 つ、`string.rb` 「RSTRING_CSTR with a non-terminated
  static string」（`Mrbtest.nofree_cstr?`）。参考列ではこれらは通る。
- テストの中身が 891d1d9d で変わったもの: `integer.rb` 「Integer#%」、`syntax.rb` 「defined? on
  constant paths (A::B)」（未調査）。

**891d1d9d で新しく入ったテスト（45）**
- 演算子の再定義が届かない（手で確かめた: `class Integer; def +(o) :redef end end; a = 1; p a + 2` が
  SabiRuby では 3、本家 rc2 では `:redef`。891d1d9d の本家は `p` の中の `+` まで置き換わって例外になる）:
  `integer.rb` 3 つ、`float.rb` 2 つ、`symbol.rb` 1 つ。`basicobject.rb` の `==` の 2 つも同じ種類と
  見ているが、未調査。
- `$!`: `exception.rb` の 3 つ。rescue 修飾子の中と ensure の中で `$!` が nil（手で確かめた。本家は
  例外を返す）。「raise without arguments re-raises the exception being rescued」は未調査。
- `defined?` の定数パスとレシーバ付きメソッド: `syntax.rb` の 7 つ。「nil is not a symbol nor a
  string」（1 つは「Object is not …」）で落ちるものが 4 つ、残りは答えが nil（未調査）。
- `syntax.rb` 「keyword arguments with fifteen or more positional parameters」（未調査）。
- メソッドを置く先とスコープ: `methods.rb` 4 つ（レシーバ付きのメソッドの中の `def`/`alias`/`undef`、
  ブロックで書いたクラス本体の `def`）、`class.rb` 1 つ、`module.rb` 10 個（`attr_*`・`define_method`・
  `class << self` の中の `private` の可視性、ブロックで渡されたクラスの中での定数と cvar の探索、
  `module_function`、`method_defined?` の inherit false。最後のものは途中で `TestNotImplement` に触れて error）。全部未調査。
- `hash.rb` の `eql?` が Hash を変える 3 つ（rc2 からある 1 つ目も今の SabiRuby では落ちている。未調査）。
- `kernel.rb` の `respond_to?` の可視性 3 つ、`__iv_set_hidden`（C fixture）1 つ。
- `gc.rb` 2 つ（`__gc_add_region_undersized` は C fixture、`GC.step_ratio=` の範囲は未調査）。
- `array.rb` 「Array#[] with a Float index that stands for no Integer」、`string.rb` 「String
  interpolation calls #to_s」（未調査）。

`refinement.rb` は両者とも「requires MRB_USE_REFINEMENTS」で skip（SabiRuby は skip 1、passed）。
計画で心配していた「refinement が無い」差は、このテストの書き方では表に出ない。

`syntax.rb` は SabiRuby だけが runtime_error: 88 個の assert の後、ファイルのトップレベルで
`TestNotImplement`（C fixture）を参照して NameError になり、残りは数えられていない。

## 条件を揃えられなかった点

- **コンパイラ**: SabiRuby は本家 `mrbc`（891d1d9d、`-g`）、mruby/edge は自前のコンパイラ。計画どおり
  それぞれの通常の使い方。行番号やローカル変数名の有無で結果が変わるテストがあるかは調べていない。
- **mruby/edge の実行環境**: libclang が無いため、同じ PC の Docker コンテナ（`rust:1.97.0-bookworm`）で
  走らせた。SabiRuby は WSL2 上で直接。上限 30 秒に近いファイルはどちらにも無い（1 ファイルの最長は
  SabiRuby 0.032 秒、mruby/edge 0.114 秒）。
- **gem の mrblib**: SabiRuby が読み込むのは今までどおり rc2 の `mrbc` で作った mrblib（`src/mrblib/`）。
  テストだけが 891d1d9d。
- **`_spec_event` の文字列化**: 文字列・整数・nil 以外の引数は mruby/edge では Rust の Debug 表記、
  SabiRuby では `inspect`。詳細の文字列だけの差で、数には関わらない。
- **`_str_match?`**: 実装が別（本家 `driver.c` の移植と、mruby/edge の書き直し）。この 48 ファイルで
  呼ばれた 18 回は答えが一致した。

## 気づいた点

- **VM（sabiruby）／本の素材**: 演算子の再定義が届かない（`Integer#+` など、`src/vm.rs` の算術の
  速い道と思われる。未確認）、`$!` が rescue 修飾子と ensure の中で nil、レシーバ付きのメソッド内の
  `def`、ブロックで渡されたクラスでの定数の探索。どれも 891d1d9d で本家にテストが入ったもので、
  rc2 の baseline を 891d1d9d に上げるなら、ここが新しい落ちになる。直すかどうかは別の計画（著者に）。
- **計画書の前提**: 「refinement が無いなどの大きな差」は、`refinement.rb` が `MRB_USE_REFINEMENTS`
  の無いビルドでは skip するので、この比較では差にならなかった。
- **本家のバージョン表記**: 891d1d9d の `mruby --version` は `mruby 4.0.0 (2026-04-20)`。master の
  `version.h` が rc2 のリリース側と別になっている。本で「master のバージョン」を書くときに注意。
- **mruby/edge の状態名**: `compile_error` に、ローダーが拒んだ場合（`rite::load` の `InvalidFormat`）も
  入る。mruby/edge 側の話で、報告するかどうかは著者の判断。
- **既存の Docker イメージの作り方**: `ref/mruby_containers/rake_nobison_install_bookworm.Dockerfile` の
  2 段目は `ARG MRUBY_VER` を宣言していないので、イメージに残るソースは既定ブランチの先頭（`mrbc`
  は 1 段目のもので正しい）。`kishima/mruby:4.1.0-rc2` の `/usr/src/mruby` は rc2 ではない可能性がある。

## 2026-10-16 追記: 公開ページを SabiRuby の互換レポートだけにした

著者の判断（2026-10-16）「SabiRuby の優位性を示したくはない」を受けて、公開する
`docs/verification/edge-compare/index.html` は SabiRuby と SabiRuby (with C fixtures) の 2 列だけの
「SabiRuby mruby compatibility report」にした。mruby/edge の数・状態・assert は出さず、`mrubyedge.json`
も読まない（`tools/edge-compare/report.py` の `report_page`）。mruby/edge の結果は載せ直さず、
mruby/edge の公式レポート（https://mrubyedge.github.io/mrubyedge/mruby-spec/）へのリンクにした。
Conditions には、テストファイル（891d1d9d の 48 ファイル、`SHA256SUMS` と同一）と driver
（`helpers.rb`、改変なし）が mruby/edge 自身の互換レポートと同じものだ、という事実だけを書き、
比較や評価の言い回しは入れていない。mruby/edge の `compile_error`（ローダーが拒む 3 ファイル）の
注は mruby/edge の結果の話なので公開ページから外した。

今までの横並びのページは `compare.html`（`report.py` の `page`、冒頭に「公開しない、index.html を見よ」の
1 行）として残し、`mrubyedge.json` があるときだけ書く。`tools/edge_compare.sh` は `mrubyedge.json` の
有無に関係なく `report.py` を呼ぶようにした（index.html はもう mruby/edge の JSON に依存しない）。
`.github/workflows/pages.yml` は `index.html`、`sabiruby.json`、`sabiruby-fixtures.json` だけを写す
（以前は `*.json` で `mrubyedge.json` も写していた）。

確認: `python3 tools/edge-compare/report.py` だけを走らせ（測り直しはしていない、JSON は 1e4b677 のまま）、
`compare.html` が以前の `index.html` と冒頭の 1 行以外同じことを diff で見た。Playwright（Chromium）で
`index.html` を 1200px / 400px、ライト / ダークで開き、全 `details` を開いた状態で横スクロールなし、
pageerror 0。
