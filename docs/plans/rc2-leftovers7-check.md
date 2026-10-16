# 2026-10-17 rc2 との小さな差の点検（方針 docs/design/compatibility.md に照らして）

調べただけ。コード・docs は触っていない。sabiruby main 28cbf01（`cargo build --release -p sabiruby-cli`）、
rc2 = Docker `kishima/mruby:4.1.0-rc2`、master = `kishima/mruby:891d1d9d`、CRuby = 手元の ruby 3.2.6。
プローブはこのスクラッチの `t/g1.rb`〜`g15.rb`、`chk/conv.rb`（l4/conv.rb の再実行、`chk/cat.rb` で分類）、
`deadnat/`（使われないネイティブを数える使い捨ての Rust、`deadnat/out.txt`）、`run.sh`（4 つ並べて回す）。

分類: A = rc2 に合わせる / B = rc2 が明らかな不具合 → 正しい側を取って記録 / C = 著者が決める / D = 挙動の差でない。
「既に正しい」B は README の Known deviations と tests/custom への記録がまだ要る。

## 総括表

| # | 項目 | rc2 | SabiRuby（28cbf01） | master / CRuby | 分類 | 直す大きさ | 危険・熱い道 |
|---|---|---|---|---|---|---|---|
| 1 | `sort`／`sort!`／`sort_by` の比較の失敗 | ArgumentError "comparison failed"（`src/array.c:2374`）。`max`／`min` は "comparison of X with Y failed" | "comparison of String with Integer failed" | master 同じ／CRuby は X with Y | A（文言は例外にしない） | `array.rs:99`、`ext_array.rs:102` の 2 か所 | 低。比較の失敗の腕だけで熱い道ではない |
| 2 | `values_at` に Integer・Range 以外（`"a"`、nil、1.5、NaN） | TypeError "invalid values selector: a"（`src/range.c:458`）。1.5 も拒む | "no implicit conversion of String into Integer"、1.5 は `[2]` | master 同じ／CRuby は Sabi 寄り | A | `ext_array.rs:207`、`ext_struct.rs`（Struct#values_at も同じ差を確認）の 2 か所 | 低 |
| 3 | `raise C, "m", bt`（3 引数） | ArgumentError "wrong number of arguments (given 3, expected 0..2)"（`fail`、`Kernel.raise` も） | 受け付けるが **bt は使わない**（backtrace は実際の位置。worklog の「bt を backtrace にする」は不正確） | master 同じ／CRuby は bt を使う | A | `kernel.rs:199` の `argc!(vm, a, 0, 3)` を 0..2 に、1 行 | 低。rubevy・games・playground に 3 引数の raise なし |
| 4 | `private [:a]`／`private "a"`／`private 1`（`public`・`protected`・`module_function` も） | TypeError "wrong argument type Array (expected Symbol)"（String・Integer・nil も同じ形、`mrb_check_type(..., MRB_TT_SYMBOL)`） | Array を展開、String を受け付ける、Integer・nil は "1 is not a symbol nor a string" | **master は Array を受け付ける**（5e77136c3、`attr_*` が名前を返すのと組の機能追加。不具合の修正ではない）／CRuby は受け付ける | A（master の変更は機能追加なので追わない） | `object.rs:624` `flat_syms` とその 3 か所の呼び手 | 低。`private [` `private "` の使用は rubevy・games に無い |
| 4b | （新）`private :a` などの返り値 | **self（クラス）**。`private`（引数なし）も self（`mrb_mod_private`／`top_private` が mod を返す） | CRuby 形: `:a`／`[:a, :b]`／nil | master 同じ／CRuby は Sabi と同じ | A | 同じ 4 つのメソッド | 低（返り値を使う書き方は見当たらない） |
| 5 | `private_class_method`／`public_class_method` | NoMethodError（rc2 に無い） | ある（`object.rs:102-103`） | master も無い／CRuby はある | A（入っていない機能） | 2 行消す | 低。rubevy・games・playground・tests に使用なし。coverage.md の行が消える |
| 6 | `Struct#hash` | `Enumerable#hash`（mrblib/enum.rb、`Array#hash` も owner は Enumerable）なので `S.new(1,2).hash == [1,2].hash` | 違う（Struct は mrblib の Enumerable#hash、Array は `array.rs:274` の独自の式）。`[].method(:hash).owner` も Array | master 同じ／CRuby は違う | **C** | Array#hash を `__update_hash` と同じ式にする（1 か所）か、Struct 側を Array の関数に寄せる | Array#hash は Array をキーにした Hash の熱い道。rc2 の式（xor とずらし）は衝突しやすい（`[2,0]` と `[0,1]` 系） |
| 7 | 名前の無いクラスの文言（`Regexp.union` ほか） | `%Y`: 無名クラスは `#<Class:0x…>`、値は "Integer cannot be converted to String" | 一般の `%Y` は rc2 と一致。差は **`Vm::expect_str` が引数のラベル**（"pattern"、"separator"、"argument"、"value"、"string"、"format" …）を書くこと: `Regexp.union(1)` "pattern cannot…"、`[1].join(1)` "separator cannot…"、`"a".include?(1)`、`index`、`replace`、`s[0]=1`、`Regexp.escape`、`Set#join` | master 同じ | A | `vm.rs:1429` の `expect_str` 1 関数（呼び手 42 か所はそのまま）。`describe_for_type_error` を使う | 低。tests・verification にラベルの文言を待つものは無い。エラーの腕だけ |
| 7b | （新）`"a" + nil` | "nil cannot be converted to String" | "NilClass cannot be converted to String" | | A | 1 か所 | 低 |
| 7c | （新）`Object.const_get(1)` | "Integer cannot be converted to String" | "1 is not a symbol nor a string" | | A | `object.rs:470` 付近 1 か所 | 低 |
| 8 | 整数の引数の変換（`expect_int` と `mrb_as_int`） | 下の節 | 66 メソッドで差（4 か月前の 68 から `Integer()`、`delete_at` が直った） | | A（例外 2 つは別行 9・10） | 文言だけの 41 メソッドは `vm.rs:1435` `expect_int` の誤りの腕 1 関数で大半が揃う。値の差 25 は個別に約 12 か所 | `expect_int` は呼び出しの多い関数。変えるのは冷たい腕だけだが、rc2-leftovers4 で冷たい関数の文言で 2〜4% 動いた前例があるのでベンチ要 |
| 9 | `Integer#to_s(x)` に Integer 以外 | `int_to_s` が `mrb_integer(arg)` を型を見ずに読む: nil・`"1"`・2**70 → "invalid radix 0"、1.5 → "invalid radix 4602678819172646913"、NaN も Float のビット列 | 型の例外（"no implicit conversion of nil into Integer"） | master 同じ／CRuby は "invalid radix 1"（1.5）、nil は TypeError | **B**（値の表現（boxing）しだいで答えが変わる＝未定義の読み） | 今のままでよい（文言を `mrb_as_int` 形に揃えるかは 8 と一緒に）。README と tests/custom に記録 | なし |
| 10 | `Array#[]`・`slice`・`[]=` に NaN・±Inf・1e30 | NaN → nil（`(mrb_int)NaN` は C の未定義動作、x86-64 で MRB_INT_MIN）。`a[NaN]=1` は "index -9223372036854775808 out of array" | **rc2 の x86-64 の答えを写している** | **master e2148c1b1 が不具合として直した**: RangeError "float NaN out of range"／"float Infinity out of range"／1e30 は "integer out of range"。CRuby も RangeError | **B**（未定義動作＋本家が後で直した） | 添字の Float の腕（`aget_index` 相当）1〜2 か所＋README＋custom テスト | Float の腕だけ。`OP_GETIDX` の Int の道には触れない |
| 11 | `rand(NaN)`／`rand(Inf)`／`Random#rand(Inf)` | `(mrb_int)mrb_float(arg)` の未定義動作。NaN → Integer、Inf → Integer | NaN → **Float**（`rand(0)` 扱い）、Inf → 大きな Integer。どちらとも違う | **master e2148c1b1**: RangeError "float NaN out of range"。CRuby は FloatDomainError | **B** | `ext_random.rs` の Float の腕 1 か所＋記録 | 低 |
| 12 | `Float#floor(n)`／`ceil(n)`／`truncate(n)` | 桁を読む: `1.25.floor(1)` 1.2、`15.5.floor(-1)` 10、nil は TypeError | **引数を無視**（1、2、1 を返す） | master・CRuby とも rc2 と同じ | A（機能の欠け） | `numeric.rs:647-649` の 3 つ（`Float#round` の桁の処理を流用できる） | 引数なしの道に長さの検査が 1 つ増えるだけ。rubevy・games の .rb に floor/ceil の使用なし |
| 13 | `Array#pop(n)` | ArgumentError（rc2 の pop は引数を取らない） | `[1,2].pop(1)` → `[2]` | master 同じ／CRuby は受ける | A（入っていない機能） | `array.rs:258` 1 か所 | 低（`.pop(` に引数を渡す .rb なし。rubevy の `q.pop(timeout_ms:)` は Queue） |
| 14 | `Integer#<<`／`>>` に Float | 1.5 を 1 と読む（`1 << 1.5` → 2） | "can't convert Float into Float"（文言自体が壊れている） | master・CRuby とも 2 | A | `numeric.rs:494` `shift` の腕 1 か所 | 誤りの腕だけ |
| 15 | `Integer.__ensure` の 2**70・NaN（`Enumerable#first`／`take`／`each_slice`／`each_cons`、`Range#first`、`Hash#first`、`Array#each_slice`、`String#lines`） | RangeError "integer out of range"／"float NaN out of range" | 2**70 を通す、NaN は "can't convert Float into Integer" | | A | `numeric.rs:529` 1 か所で 8 メソッド分 | 低 |
| 16 | `[x].shuffle(random: 1)`／`sample(random:)` | 長さ 1 以下なら検査しない（`mrb_ary_shuffle_bang` の `if (RARRAY_LEN(ary) > 1)` の中で検査） | 常に TypeError "Random object required" | master 同じ／CRuby は `[1]` で誤り、`[]` で通る | A（小さい。後回しでよい） | `ext_random.rs` 1〜2 か所 | 低 |
| 17 | 凍結した特異クラス（`o.singleton_class.freeze`、`o` は凍結していない） | `def o.x`／`define_method` は**通る**（`mt_writable` が特異クラスなら付いている物の凍結を見る。ccdf75c1c、2023 Matz）。ただし `o.extend(M)` は FrozenError | どれも FrozenError | master 同じ／CRuby はどれも FrozenError | **C**（勧めは B） | 合わせるなら定義の口（`def_method_raw`、`define_method`、可視性の変更）で 2〜3 か所 | 低。`singleton_class.freeze` の使用なし |
| 18 | `eval("__method__")`（top level）、メソッドの中の `eval("super(...)")`、`eval("defined?(super)")` | `:eval`、`nil`（super が `Kernel#eval` の親を探す）、`"super"` | `nil`、`[:base, 11]`、`nil`（既に正しい側） | **master b0b1de54e が不具合として直した**（答えは Sabi と同じ）。CRuby も同じ | **B**（既に正しい。記録が無い） | コードは変えない。README の Known deviations に 1 項、tests/custom に 1 本。backlog の「`__method__`」の行から eval の分を外す | なし |
| 19 | `instance_exec { __method__ }`／`class_exec`（top level） | `:instance_exec`／`:class_exec`（`instance_eval` は nil） | nil | master も rc2 と同じ／CRuby nil | **C**（勧めは B） | 合わせるならフレームの形を変える（backlog の既存の判断どおり重い） | フレームの形。熱い道 |
| 20 | 使われないネイティブ（mrblib に上書き） | — | 37 個: Array の count・deconstruct・delete_if・dig・fetch・fill・keep_if・product・reject!・select!・sort・sort_by・to_h・transpose・uniq・uniq!、Hash の delete・dig・fetch・invert・merge・merge!・to_h・update、Range の first・hash・last、Integer#succ、Numeric の +@・abs、Proc#to_proc、Complex#to_c、String の chars・each_byte・each_char・lines・upto・__upto_endless（`deadnat/out.txt`、既定の機能で `Vm::new` 直後と `load_mrblib` 後を比べた） | | D | 表の行を消すだけ。消してよいことは `deadnat` で機械的に確かめられる | なし（`Vm::new()` だけで mrblib を読まない使い方は tests/malformed_rite.rs のみ） |
| 20b | 使われないネイティブ（同じ名前のネイティブに上書き） | — | 約 45 個: `object.rs` の Object の object_id・public_send・methods・public_methods・private_methods・protected_methods・singleton_methods（Kernel に同名があり `ext_metaprog` の掃除で消える）と Module の class_variable_get/set・constants・instance_methods・private/protected/public_instance_methods・remove_method（`ext_metaprog` が上書き）、`string.rs` × `ext_string.rs` 18（<<、center、clear、concat、count、delete、end_with?、inspect、ljust、lstrip、next、ord、rjust、rstrip、start_with?、strip、succ、tr）、`hash.rs` × `ext_hash.rs` 3（__merge、key、values_at）、`range.rs` × `ext_range.rs` 2（cover?、size）、`numeric.rs` pow、`proc_.rs` × `ext_proc.rs` 4（inspect、lambda?、parameters、to_s）、`kernel.rs` local_variables | | D | 表の行を消す。`string.rs` の `[]`・`index`・`split` など 12 個は regexp なしのビルドで生きるので**消さない** | なし |
| 21 | `FromRuby for Vec<T>`（`convert.rs:148`） | rc2 の `mrb_get_args("A")` は Struct を拒む | `ary_vals` で Struct・Data を Array として受け取る。文言 "no implicit conversion into Array" | | D（ホストの API） | rustdoc は "An Array" と言っているので `plain_ary_vals` にし、文言を "%Y cannot be converted to Array" に（1 か所）。公開 API の挙動が変わるので CHANGELOG に 1 行 | rubevy・games の .rb に Struct・Data の使用なし。Rust 側で `Vec<T>` を受けるホスト関数に Struct を渡す例も無い |
| 22 | `Vm::ivar_set` が iv を持てない値にも書ける | — | rustdoc に記載済み（rc2-ivar） | | D | 済み。今のままでよい | — |

既に直っていたもの（今回走らせて rc2 と一致を確認）: `remove_instance_variable` の名前の検査、多重代入の `to_ary`、
`instance_variable_names`、Regexp の iv の順、`Integer(2**70)`、iv を持てない値への `instance_variable_set`、
Struct・Data を Array として扱う Array のメソッド（`[1]+s`、`[s].flatten`、`[1,2]==s`、`Array(s)`）、`Array#delete_at` の変換。

著者が既に決めた行（今回は判断し直していない）: `instance_variables` の順（合わせない、README 記載済み）、
Hash の小さい表の `eql?`（案 (a)）、「hash modified」の世代の数、backtrace の残り 7 つ。

## 8. 整数の変換の表（再実行）

`chk/conv.rb`（l4 と同じ 84 の呼び方 × nil、NaN、"1"、1.5、2**70）。差は 66 メソッド（212 行）。

文言だけ（41）: Array#at／first／last／shift／rotate／rotate!／insert／fetch／take／drop／flatten／fill／sample、
String#*／center／ljust／rjust／insert／getbyte／setbyte／to_i／index／rindex／byteindex、Integer#round／digits／ceil、
Float#round、Random.new、srand、Struct#[]、MatchData#begin／end／offset、Regexp#match、String#match(re, pos)、
Time.new、Array#pack、sprintf %d、%*d。形はどれも「"no implicit conversion of nil into Integer" ↔ "nil cannot be
converted to Integer"」と「FloatDomainError "NaN" ↔ RangeError "float NaN out of range"」。

値・例外のクラスまで違う（25）と、どこで直るか:

| 呼び方 | rc2 / SabiRuby | 直す所 |
|---|---|---|
| Array#pop | 引数を取らない / pop(n) を受ける | 行 13 |
| Array#* | 1.5 → `[1, 2]`、nil → "nil cannot be converted to Integer" / "argument cannot be converted to String" | `array.rs` の `*` |
| Array#values_at | "invalid values selector" / Integer として読む | 行 2 |
| Array#combination／permutation／dig、String#lines、Range#last（NaN） | RangeError / TypeError "can't convert Float into Integer" | 各 1 か所、または共通の変換 |
| Array#each_slice、Range#first、Enumerable#first／take／each_slice／each_cons、Hash#first（2**70・NaN） | RangeError / 通す | 行 15（`Integer.__ensure` 1 か所） |
| String#setbyte(0, 1.5) | 1 / 1.5 | 1 か所 |
| String#split(" ", 1.5) | 上限 1 / 上限なし（`"a b".split(" ", 1.5)`。2.5 は一致） | 1 か所 |
| Integer#to_s | 型を見ずに読む / 型の例外 | 行 9（B） |
| Integer#<< | 1.5 → 2 / "can't convert Float into Float" | 行 14 |
| Float#floor／ceil／truncate | 桁を読む / 引数を無視 | 行 12 |
| Kernel#rand、Random#rand（NaN） | Integer（UB）/ Float | 行 11（B、master は RangeError） |
| Array#shuffle(random: x) | 長さ 1 以下は検査しない / 常に検査 | 行 16 |

master と rc2 の差はこの表では `rand(NaN)` だけ（e2148c1b1）。

## C の理由

- **6 `Struct#hash`**: 差は「Struct の hash と同じ要素の Array の hash が等しいか」という関係と、`Array#hash` の owner。
  SabiRuby の hash の値そのものは rc2 と元から違う（`1.hash` が rc2 1048580676、Sabi 1）ので、値は合わせていない。
  合わせるには Array#hash を rc2 の `Enumerable#hash` と同じ弱い式にする（Array をキーにした Hash の速さに効く）か、
  Struct に専用の hash を足す（rc2 に無いメソッドが増える）。「hash の値は SabiRuby 独自」と README に書いて
  合わせない、が素直に見えるが、方針の「本家に合わせる」から外れるので著者の判断。
- **17 凍結した特異クラス**: rc2 は `def` を通し `extend` を拒む — mruby の中で食い違っている（方針の「内部の不整合」）。
  ただし通るのは Matz が付いている物の名前で FrozenError を出すために入れた分岐（ccdf75c1c）の副作用で、
  意図か不具合かはコードからは決められない。CRuby は全部拒む。勧めは B（今の SabiRuby のまま、記録する）。
- **19 `instance_exec { __method__ }`**: rc2・master とも `:instance_exec`、`instance_eval` は nil で食い違い、
  CRuby は nil。master は eval の分（行 18）だけ直してこちらは残した。合わせるならフレームの形を変える重い仕事
  （backlog に既にある）。勧めは B（nil のまま記録）か、backlog のまま「重いので後で」。

## 作業の副作用

`/home/kishima/book/ref/mruby` で一度 `git checkout 4.1.0-rc2` してしまい、すぐ元の HEAD（`3cf73ee4a`、4.1.0-rc の
タグの位置、detached）に戻した。作業ツリーの変更は無い。rc2 のソースはこのスクラッチの `rc2/` に `git archive` で展開して読んだ。
