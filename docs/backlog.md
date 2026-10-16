# Backlog — 決めて後に回したもの

「やらないと決めたもの」「後で決めるもの」を 1 か所にまとめる。1 項目 1 行で、**何か／なぜ後にしたか／何が起きたら見直すか／詳しい記録**を書く。
やると決めて計画書に移したら、この表から消し、移った先を書く。

作成 2026-09-26（0.7.0 の範囲を決めたとき、`plans/release-0.7-plan.md`）。

## 追従する本家のバージョン（著者の方針、2026-10-16）

| 項目 | なぜ後に | 見直すとき | 記録 |
|---|---|---|---|
| **本家 master への追従**（今の互換の基準は 4.1.0-rc2＝`stable`。master は `stable` の祖先ではなく、分岐 `4e2bc5c2b` 以降 600 近いコミットがある） | 著者の方針: 4.1 が正式リリースされ、本（『Deep dive into mruby』）を出版するまでは 4.1 の挙動を守る。master の変更は VM・mrblib・テスト・コンパイラが組で入るので、部分的には取り込まない | 4.1.0 の正式リリースと本の出版の後。その後は最新の mruby の機能に定期的に追従する（追従の間隔と手順は、そのときに計画書で決める） | `plans/follow-master-plan.md`、`worklog/2026-10-16-follow-master.md`（rc2 と 891d1d9d の挙動の違いの表） |

## VM の機能

| 項目 | なぜ後に | 見直すとき | 記録 |
|---|---|---|---|
| **段階 3: 暗黙の呼び戻しの中でも待てるようにする**（`join`/`inspect` の `to_s`、`include?` の `==`、Hash の `hash`/`eql?`、`sort` の `<=>`、ホストの関数からの呼び戻し） | ネイティブのフレームを再開できる形（状態機械か `core::future`）にする必要があり、約 130 か所になる。そこで待ちたい例がまだ無い | スクリプトの利用者が、そこで待ちたい場面に当たったとき | `plans/wait-anywhere-plan.md` 段階 3、`design/wait-anywhere.md`「What is still a boundary」 |
| **整数の引数の変換の文言（`Vm::expect_int` と rc2 の `mrb_as_int`）**（2026-10-16 の rc2-leftovers4 で見つかった）: rc2 の `mrb_get_args("i")` は `mrb_as_int`（"nil cannot be converted to Integer"、RangeError "float NaN out of range"）、SabiRuby の多くは `expect_int`（CRuby の "no implicit conversion of nil into Integer"、FloatDomainError）。nil・NaN・String・1.5・2**70 で 84 の呼び方を試し、68 で違った。文言だけのものが 44、値まで違うものが 24（`Float#floor(nil)` が引数を見ない、`Array#pop(1)` を rc2 は受け付けない、`Array#*` の Float、`Integer#to_s` の基数、2**70 を `first` 系が通す、など） | 呼び出しが約 110 か所あり、`expect_int` を `mrb_as_int` に替えるだけでは済まない所（`mrb_ensure_int_type`、`to_int` を見る所）が混じる。1 つずつ本家の関数を確かめる仕事になる | 次に 4.1 との差を直すとき | `worklog/2026-10-16-rc2-leftovers4.md`「整数変換の一覧」 |
| **`array.rs` の使われない 15 個**（`&`、`-`、`|`、`assoc`、`at`、`compact`、`compact!`、`flatten`、`flatten!`、`include?`、`member?`、`insert`、`rassoc`、`rotate`、`values_at`）: `ext_array::init` が同じ名前で上書きする | 計画（rc2-leftovers4）が消すとしたのは `slice!` だけ | 次に Array を片付けるとき | `worklog/2026-10-16-rc2-leftovers4.md` P2 |
| **Array のメソッドが Struct・Data を Array として受け取る**（2026-10-17 の rc2-leftovers5 で見つかった）: rc2 では Struct も Data も `MRB_TT_STRUCT` で `mrb_array_p` が偽。`[1] + s`、`[1, 2].concat(s)`、`replace(s)`、`product(s)` は rc2 が TypeError、SabiRuby は中身を使う。`[s].flatten` は rc2 が `[s]`、SabiRuby は `[1, 2]`。`[1, 2] == s` は rc2 が false、SabiRuby は true。`[3, 4] <=> s` は rc2 が nil。`Array(s)` は rc2 が `[1, 2]`（`to_a`）、SabiRuby は struct そのもの | 計画（rc2-leftovers5 R1）の範囲は多重代入と block の引数の展開だけだった。直すなら Array の各メソッドの引数の読み方を `Heap::plain_array` に替えることになり、1 つずつ rc2 の関数を確かめる仕事 | 次に 4.1 との差を直すとき | `worklog/2026-10-17-rc2-leftovers5.md`「気づいた点」 |
| **`instance_variables` の順（OBJECT 以外）**: rc2 は普通のオブジェクト（OBJECT）だけ shape で書いた順を保ち、Hash・Exception・CDATA（Regexp、Time など）・クラスは `iv_tbl` をシンボルの番号の順に並べる（`src/variable.c` の二分探索）。`{}` に `@source`、`@flags` の順で置くと rc2 は `[:@flags, :@source]`、SabiRuby は置いた順。`super` の前に自分の iv を置く Regexp のサブクラスも違う | 写すには rc2 と同じシンボルの番号（presym の順と intern の順）が要り、SabiRuby のシンボル表は作りが違う。R3 では Regexp 自身の 2 つの順だけ合わせた | 利用者がこの順に頼る例が出たとき | `worklog/2026-10-17-rc2-leftovers5.md` R3 |
| **`object.rs` の使われない `instance_variable_get`／`set`／`defined?`／`instance_variables`**: `ext_metaprog::init` が名前の検査つきの同じ名前で上書きする | 計画（rc2-leftovers5）の範囲の外 | 次に Kernel を片付けるとき | `worklog/2026-10-17-rc2-leftovers5.md` R2 |
| **`sort { }` の比較の中で待つ** | 著者の判断（2026-09-26）: 軽い block で 8.1% 遅くなるので払わない | 段階 3 をやるとき | `verification/bench.md`「After the author's decisions」 |
| **Hash の小さい表でもハッシュ値の違うキーに `eql?` をかける**（rc2 の `hash` の assert、`worklog/2026-09-23-rc2.md` の案 (b)） | 0.7.0 の S1 で案 (a) を取った（2026-09-26）。(b) は探索の意味を表の大きさで変える本家の都合を真似ることになり、CRuby とも違う。探索の速さ（stage 2c）にも触れる | 本家が AR でもハッシュ値を比べるようになったとき、または利用者がこの違いに当たったとき | `worklog/2026-09-26-release-0.7-a.md`、`verification/mrbtest-notes.md` の `hash` |
| **「hash modified」を世代の数で見る**（今は要素数だけ。`eql?` の中で 1 つ消して 1 つ足すと気づかない） | Hash を書き換える道（`[]=`、`delete`、`rehash`、`replace`、`clear`、`shift`、`compact!`、`select!` ほか）すべてで数を進める必要があり、1 つ漏れると検査が黙って効かなくなる。今でも範囲外は読まない（Rust の `Vec`）ので、困るのは本家と答えが違うことだけ | 本家のテストか利用者が、数の変わらない書き換えで違いに当たったとき | `worklog/2026-09-26-release-0.7-a.md` 気づいた点 5 |
| **`eval` と `binding` の irep を返さない**（`ext_eval.rs`、`ext_binding.rs` の `wrap_lvspace`。呼ぶたびに `ireps` が伸びる） | 番号を誰にも渡さないので、いつ使われなくなったかは回収（GC）でしか分からない。GC に irep の印と掃除を足すことになり、`Vm::unload` の「GC の規則を変えない」を崩す | スクリプトが `eval` や `binding` をループの中で繰り返し呼ぶ例が出たとき | `worklog/2026-09-26-release-0.7-b.md` 気づいた点、`design/gc.md`「Programs」 |
| **backtrace の残りの違い 7 つと、`instance_exec`・top level の `eval` の中の `__method__`**（本家は `:instance_exec`／`:eval`、ここは `nil`） | どちらも本家が C 関数のフレームを書き換えて名前を運ぶ形で、揃えるには `CallInfo` に 1 つ足すか `Cci` を増やす — フレームの形を変える | フレームの形を変える仕事（段階 3 の再開など）が来たとき、または利用者がこの違いに当たったとき | `worklog/2026-09-26-release-0.7-b.md` S5「残った違い」・気づいた点、`tests/custom/native_backtrace.rb`、`design/gems.md` |
| **記録のイベント（`TraceEvent::Raise`／`CatchLook`）が `unload` した irep を指しうる** | 表示にしか使わず、行が `None` になるだけで壊れない | デバッガがこの行を必要としたとき | `worklog/2026-09-26-release-0.7-b.md` 気づいた点（`src/inspect.rs`） |
| **`load_and_run` で読んだプログラムは `unload` できない** | 番号を返さない口なので当然。起動時に 1 度読む層やデータなら困らない。何度も読み直すのは rubevy（`ScriptWorld::load_and_run`）と Factory の側で `load` + `run_irep` + `unload` に替える話 | rubevy・games が同じものを何度も読み直すとき | `worklog/2026-09-26-release-0.7-b.md` 気づいた点 |
| `gc_step(work)` とヒープの上限 | 設計の判断が要る（rubevy の outlook で「to do」） | ゲーム側で GC の停止時間かメモリの上限が問題になったとき | rubevy `docs/outlook.md:183-184` |

## 公開 API

| 項目 | なぜ後に | 見直すとき | 記録 |
|---|---|---|---|
| H3: ホストが数を数える公開の口（irep の数、ready のタスク数、この run で走ったタスク数）。今は `#[doc(hidden)] Vm::ireps` | 新しい API で、欲しいのは rubevy の `FrameStats` だけ | rubevy が `FrameStats` に足したくなったとき | `plans/host-scale-plan.md` H3 |
| H4: `Sym` で ivar を読み書きする口 | 新しい API。効くのは rubevy の publish があふれたときだけ（+10〜14%） | rubevy の publish の速さが問題になったとき | `plans/host-scale-plan.md` H4 |
| `Vm::current_line` の名前 | 名前の問題だけ | 次に API を整理するとき | `worklog/2026-09-21-serde-lines.md:418` |

## 同梱物・道具

| 項目 | なぜ後に | 見直すとき | 記録 |
|---|---|---|---|
| `.rbs`（`sig/`）の同梱 | VM の振る舞いに関係しない。型を使う利用者がまだいない | 型検査を使う利用者が出たとき | `plans/from-mrubyedge-plan.md:15` |
| eval の `capture_errors` = FALSE の TODO | 本家と同じ振る舞いで困っていない | eval の誤りの報告を良くしたくなったとき | `plans/eval-require-plan.md:72` |
| README の数（fixtures の本数、mrbtest の通過数、gem の数など）を CI で数え直す | 0.7.0 の S7 で見送った（2026-09-26）。いちばん古くなりやすい mrbtest の数は本家の `mrbc`（Docker）で作った表から出るので CI では数え直せず、数え直せる数だけ見ると「確かめた」ように見えて一番ずれやすいものが漏れる | README の数のずれがまた見つかったとき、または mrbtest の表を CI で作れるようになったとき | `worklog/2026-09-22-readmes.md` 気づいた点 3、`worklog/2026-09-26-release-0.7-a.md` |
| wasm の大きさを測っていない | playground の読み込みが問題になっていない | wasm の大きさが問題になったとき | `plans/gems-plan.md:216` |

## 本家（mruby）への報告

| 項目 | なぜ後に | 見直すとき | 記録 |
|---|---|---|---|
| mruby-task の報告候補 4 件（終わったタスクが VM の寿命まで残る、ほか） | 著者の判断（2026-09-17）: 記録だけ | 著者が送ると決めたとき | `verification/upstream-pr-candidates.md`、`plans/upstream-task-plan.md` |
| 境界の内側の `sleep(n)` の後、そのタスクが横取りされなくなる件と、タスクの中の Fiber で異常終了する件 | 記録だけ（2026-09-26） | 同上 | book の `docs/notes/upstream-reports.md` の候補 10・11 |

## 性能（原因が分からないまま受け入れたもの）

| 項目 | なぜ後に | 見直すとき | 記録 |
|---|---|---|---|
| 0.7.0 で block なしの整数の `sort` のマイクロベンチが +9%、rubevy の `publish_heard`（購読 10 以上・1 フレーム 100 通以上）が +11〜15% | 著者の判断（2026-09-27）: 現状で受け入れる。S4・S5・S6 のどれでもなく、命令数は同じ、VM 単体の同じ操作は遅くない。深追いしない | `perf` が使えるようになったとき、または実際のゲームで publish の遅さが問題になったとき | `verification/bench.md` の 0.7.0 の節、`worklog/2026-09-27-release-0.7-bench.md` |
| rc2-leftovers（L1〜L5）で `call_kwargs` +5.4%／+4.6%、`bm_ao_render` +3.1%／+3.2%、`app_json_hash` 約 +2.5%（全体は −0.2%／−0.5%） | 著者の判断（2026-10-16）: これまでもあった範囲の揺れとして受け入れる。`call_kwargs` は定数を引かず、変更を 1 つずつ外しても原因が出ない。`codegen-units = 1` での配置のずれと見ているが、WSL では `perf stat` が使えず確かめていない | `perf` が使えるようになったとき | `worklog/2026-10-16-rc2-leftovers.md`「定数を使わないベンチマークの 2〜5%」 |
| rc2-leftovers4 で `app_tak` +3〜4%、`bm_fib` +2〜3%、`call_args` +3%（27 本の合計は +1%）。原因は `Vm::frozen_error` の文言を rc2 の形にした 1 か所で、この関数はベンチでは呼ばれない | 著者の判断（2026-10-17）: 受け入れる。旧い本体に戻すと差が消え、`#[cold]`・`#[inline(never)]` では消えなかった。コードの配置のずれと見ているが確かめていない | `perf` が使えるようになったとき | `worklog/2026-10-16-rc2-leftovers4.md` の性能の節 |
