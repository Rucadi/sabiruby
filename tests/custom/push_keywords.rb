# Keyword arguments given to a native method that takes none (rc2-sweep cause `kw-dropped`;
# `Array#push` is where it was found). mruby 4.1.0-rc2 has no single rule for the keyword Hash
# of such a call. `mrb_get_args` (`get_args_v`, src/class.c:1554-1577) counts a non-empty one as
# one more positional argument, so `"a".center(k: 1)` is a TypeError there as here. But
# `mrb_get_argc` and `mrb_get_argv` (src/class.c:1249, 1277) count only the positional
# arguments, and a method that reads them first drops the Hash without an error: `[].push(1,
# k: 2)` is `[1]` (`mrb_ary_push_m`, src/array.c:852), `[1].first(k: 1)` is 1 (`mrb_ary_first`,
# :1519), `1.to_s(k: 1)` is "1" (`int_to_s`, src/numeric.c:2125); `[1][k: 1]` asks
# `mrb_get_argc` first and `mrb_get_args` next and says "given 1, expected 2" (`mrb_ary_aget`,
# src/array.c:1369, 1387). Master 891d1d9d answers all of these the same way. Arguments
# disappearing without an error, by how the method happens to read them, is inconsistent within
# mruby itself, so SabiRuby passes the Hash on as the last positional argument everywhere, as
# CRuby does (the author's decisions of 2026-10-18: push first, then the other methods with plan
# rc2-sweep-fixes).
# expected-from: SabiRuby (CRuby 3.2 answers the same, in its own words). The reference's answers are in .rc.out.
# README, "Known deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end
t("[].push(1, k: 2)") { [].push(1, k: 2) }
t("[1].push(k: 1)") { [1].push(k: 1) }
t("[].push({k: 2})") { [].push({k: 2}) }
t("[].push(1, **{})") { [].push(1, **{}) }
t("[].unshift(1, k: 2)") { [].unshift(1, k: 2) }
t("[1].first(k: 1)") { [1].first(k: 1) }
t("1.to_s(k: 1)") { 1.to_s(k: 1) }
t("[1][k: 1]") { [1][k: 1] }
t("Struct#a(k: 1)") { Struct.new(:a).new(1).a(k: 1) }
t("Method#call(k: 1)") { 1.method(:to_s).call(k: 1) }
# methods that read their arguments with mrb_get_args answer the same on both
t("'a'.center(k: 1)") { "a".center(k: 1) }
t("[1, 2].rotate(k: 1)") { [1, 2].rotate(k: 1) }
