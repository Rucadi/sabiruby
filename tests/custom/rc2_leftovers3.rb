# `Array#[]` and the readers it shared its argument handling with (docs/plans/
# rc2-leftovers3-plan.md, N1). Expectation: mruby 4.1.0-rc2 (.rc.out).
# `mrb_ary_aget` (src/array.c): a Range goes through `mrb_range_beg_len(..., TRUE)`
# (src/range.c), whose ends are `mrb_as_int` (a Float is truncated, NaN and the infinities are
# RangeError "float NaN out of range", a value past the Integer range "integer out of range",
# a Rational truncates, anything else "X cannot be converted to Integer"). A lone index that is
# no Integer goes through `aget_index`: a Float is the C cast there, anything else
# `mrb_as_int`. The cast is undefined for NaN, the infinities and 1e30 (rc2 on x86-64 finds
# nothing); mruby master e2148c1b1 fixed it as a bug with `mrb_as_int`, and those five lines
# (`ary[NaN]`, `ary[INF]`, `ary[-INF]`, `ary[1e30]`, `ary[NaN, 1]`) expect its RangeError
# (README, "Known deviations"; float_index_out_of_range.rb). With two arguments, `mrb_get_args(mrb, "oi")` converts
# the length first. `String#[]` (`str_convert_range`, src/string.c) reads a Float index with
# `mrb_as_int` instead, and the index before the length; `String#[]=` and `byteslice` read
# their arguments the same way, and `slice!` of both (array-ext, string-ext) through
# `mrb_as_int` too.
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue Exception => e
  puts "#{label} !! #{e.class}: #{e.message}"
end
class Idx; def to_int; 1; end; end
ONE = [
  ["1.0..2.0", 1.0..2.0], ["1.5..2.7", 1.5..2.7], ["1..2.9", 1..2.9], ["1.9..", (1.9..)],
  ["..2.5", (..2.5)], ["1...3.5", 1...3.5], ["-2.5..-1.2", -2.5..-1.2],
  ["1..INF", (1..Float::INFINITY)], ["1..-INF", (1..-Float::INFINITY)], ["1e30..2", (1e30..2)],
  ["1r..2r", (1r..2r)], ["(2**64)..3", ((2**64)..3)], ["'a'..'b'", "a".."b"],
  ["nil..nil", (nil..nil)], ["nil", nil], ["true", true], [":a", :a], ["Idx", Idx.new],
  ["[]", []], ["1.5", 1.5], ["-1.5", -1.5], ["NaN", Float::NAN], ["INF", Float::INFINITY],
  ["-INF", -Float::INFINITY], ["1e30", 1e30], ["2**64", 2**64], ["1r", 1r],
]
TWO = [
  [1, nil], [nil, 1], [nil, "x"], [1, "x"], [1.5, 2.5], [1, 2.9], [Float::NAN, 1],
  [1, Float::NAN], [1, Float::INFINITY], [2**64, 1], [1, 2**64], [1, -1], [5, 1], [6, 1],
  [-6, 1], [1, 1r], [1..2, 1],
]
[["ary", proc { [1, 2, 3, 4, 5] }], ["str", proc { "abcde" }]].each do |name, mk|
  ONE.each do |l, v|
    t("#{name}[#{l}]") { mk.call[v] }
    t("#{name}.slice!(#{l})") { o = mk.call; r = o.slice!(v); [r, o] }
  end
  TWO.each do |x, y|
    t("#{name}[#{x.inspect}, #{y.inspect}]") { mk.call[x, y] }
    t("#{name}.slice!(#{x.inspect}, #{y.inspect})") { o = mk.call; r = o.slice!(x, y); [r, o] }
  end
end
[[1.5], [Float::NAN], [nil], [1.5..2.5], [1.5, 1.5], [nil, "x"], ["x", nil], [1, nil],
 [1..2, 1], [1, Float::NAN], [1e30], [1, 1e30], [1r], [1, 1r]].each do |args|
  t("s#{args.inspect} =") { s = "abcde"; s[*args] = "X"; s }
  t("byteslice#{args.inspect}") { "abcde".byteslice(*args) }
end
# a Range end through `mrb_as_int` everywhere `mrb_range_beg_len` is used
t("values_at") { [1, 2, 3].values_at(1.5..nil, "a".."b") }
t("a[1..1e30] =") { a = [1, 2]; a[1..1e30] = 9; a }
