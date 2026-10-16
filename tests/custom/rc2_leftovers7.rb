# The small differences from mruby 4.1.0-rc2 left after rc2-leftovers6 (docs/plans/
# rc2-leftovers7-plan.md, T1; the check table is rc2-leftovers7-check.md). Expectation: mruby
# 4.1.0-rc2 (.rc.out).
# `sort_cmp` (src/array.c): without a block a pair `mrb_cmp` cannot order is "comparison
# failed", a block's nil `cmpint`'s "comparison of %T with %T failed". `mrb_get_values_at`
# (src/range.c): a selector that is neither an Integer nor a Range inside the receiver is
# "invalid values selector: %v". `mrb_f_raise` takes at most two arguments. `mrb_mod_visibility`
# and `mrb_mod_module_function` take Symbols only (`mrb_check_type`) and answer the module;
# main's `public`/`private`/`protected` (`top_private`) answer Object. `private_class_method`
# and `public_class_method` are not in mruby. `mrb_ensure_string_type` says "%Y cannot be
# converted to String", `mrb_as_int` "%Y cannot be converted to Integer" and "float NaN out of
# range". `flo_rounding`: `floor`/`ceil`/`truncate` take a digit count. `mrb_ary_pop` takes no
# argument; `int_lshift` truncates a Float width; `Integer.__ensure` is `mrb_ensure_int_type`;
# `mrb_ary_shuffle_bang` reads its arguments only for two elements or more; `mrb_str_setbyte`
# answers the byte; `mrb_str_split_m` answers the receiver for a limit of 1; `mrb_str_to_i`
# hands base 0 to `mrb_str_len_to_integer`, which reads a prefix; `mrb_ary_times` reads a
# count that is no String with `mrb_as_int`.
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue Exception => e
  puts "#{label} !! #{e.class}: #{e.message}"
end
class N; def <=>(o); nil; end; end
class F; def initialize(v); @v = v; end; attr_reader :v; def <=>(o); 1.5; end; end
class Named; end
S3 = Struct.new(:a, :b, :c)
NAN = Float::NAN
# sort
t("sort int/str") { [1, "a"].sort }
t("sort! int/str") { [1, "a"].sort! }
t("sort nan") { [1.0, NAN].sort }
t("sort <=> nil") { [N.new, N.new].sort }
t("sort <=> 1.5") { [F.new(1), F.new(2)].sort.map(&:v) }
t("sort_by int/str") { [1, "a"].sort_by { |x| x } }
t("sort block nil") { [1, 2].sort { nil } }
t("sort block 1.5") { [1, 2, 3].sort { |a, b| (b - a).to_f } }
t("max int/str") { [1, "a"].max }
# values_at
s3 = S3.new(1, 2, 3)
["a", nil, 1.5, NAN, :a, [1], s3, -9..1, 5..6, 2**70].each do |v|
  t("values_at #{v.inspect}") { [1, 2, 3].values_at(v) }
  t("Struct#values_at #{v.inspect}") { s3.values_at(v) }
end
# raise
t("raise 3") { raise RuntimeError, "m", ["bt"] }
t("fail 3") { fail RuntimeError, "m", ["bt"] }
t("Kernel.raise 3") { Kernel.raise RuntimeError, "m", ["bt"] }
# visibility
def top_a; end
def top_b; end
t("top private :a") { private(:top_a) }
t("top public :a, :b") { public(:top_a, :top_b) }
class Vis
  def a; end
  def b; end
  $vis = [private(:a), public(:a, :b), protected(:a), private, public, protected].map { |x| x.equal?(Vis) ? :Vis : x }
  $vis_err = [[:a], "a", 1, nil, 1.5, true].map { |x| begin; private(x); rescue => e; "#{e.class}: #{e.message}"; end }
  $vis_err << (begin; private(:a, "b"); rescue => e; "#{e.class}: #{e.message}"; end)
end
t("Vis answers") { $vis }
t("Vis errors") { $vis_err }
module ModF
  def a; end
  def b; end
  $mf = [module_function(:a), module_function(:a, :b)].map { |x| x.equal?(ModF) ? :ModF : x }
  $mf_err = [[:a], "a"].map { |x| begin; module_function(x); rescue => e; "#{e.class}: #{e.message}"; end }
  $mf << (module_function.equal?(ModF) ? :ModF : :other)
end
t("module_function answers") { $mf }
t("module_function errors") { $mf_err }
t("private_class_method") { Class.new { def self.a; end; private_class_method :a } }
t("public_class_method") { Class.new { def self.a; end; public_class_method :a } }
# String arguments
[1, nil, :a, Named.new].each do |v|
  d = v.is_a?(Named) ? "Named" : v.inspect
  t("join #{d}") { [1, 2].join(v) }
  t("include? #{d}") { "a".include?(v) }
  t("index #{d}") { "a".index(v) }
  t("replace #{d}") { "a".replace(v) }
  t("[]= #{d}") { s = "ab"; s[0] = v; s }
  t("Set#join #{d}") { Set[1, 2].join(v) }
  t("String#+ #{d}") { "a" + v }
  t("const_get #{d}") { Object.const_get(v) } unless v.is_a?(Symbol)
end
# Integer arguments (`mrb_as_int`)
[nil, NAN, "1", 1.5, 2**70].each do |x|
  d = x.inspect
  t("Array#at(#{d})") { [1, 2].at(x) }
  t("Array#first(#{d})") { [1, 2].first(x) }
  t("Array#rotate(#{d})") { [1, 2].rotate(x) }
  t("Array#fetch(#{d})") { [1, 2].fetch(x) }
  t("Array#*(#{d})") { [1, 2] * x }
  t("Array#pop(#{d})") { [1, 2].pop(x) }
  t("Array#combination(#{d})") { [1, 2].combination(x).to_a }
  t("Array#each_slice(#{d})") { [1, 2].each_slice(x).to_a }
  t("String#*(#{d})") { "a" * x }
  t("String#center(#{d})") { "a".center(x) }
  t("String#getbyte(#{d})") { "ab".getbyte(x) }
  t("String#setbyte(0, #{d})") { "ab".setbyte(0, x) }
  t("String#to_i(#{d})") { "11".to_i(x) }
  t("String#split(' ', #{d})") { "a b".split(" ", x) }
  t("String#lines first(#{d})") { "ab".each_char.first(x) }
  t("Integer#round(#{d})") { 15.round(x) }
  t("Integer#<<(#{d})") { 1 << x }
  t("Float#round(#{d})") { 1.5.round(x) }
  t("Float#floor(#{d})") { 1.5.floor(x) }
  t("Float#ceil(#{d})") { 1.5.ceil(x) }
  t("Float#truncate(#{d})") { 1.5.truncate(x) }
  t("Random.new(#{d})") { Random.new(x).class }
  t("Range#first(#{d})") { (1..3).first(x) }
  t("Enumerable#take(#{d})") { {a: 1}.take(x) }
  t("Enumerable#each_cons(#{d})") { {a: 1}.each_cons(x).to_a }
  t("Struct#[](#{d})") { S3.new(1, 2, 3)[x] }
  t("sprintf %d(#{d})") { sprintf("%d", x) }
  t("[1].shuffle(random: #{d})") { [1].shuffle(random: x) }
  t("[1].sample(random: #{d})") { [1].sample(random: x) }
end
# Float#floor/ceil/truncate with digits
[1.25, -1.25, 15.5, 34567.89, 0.0, -0.0, 1e20].each do |f|
  [1, 2, -1, -2, 16].each do |n|
    t("#{f}.floor(#{n})") { f.floor(n) }
    t("#{f}.ceil(#{n})") { f.ceil(n) }
    t("#{f}.truncate(#{n})") { f.truncate(n) }
  end
end
t("1.5.floor(-400)") { 1.5.floor(-400) }
t("Float::INFINITY.floor(1)") { Float::INFINITY.floor(1) }
# the rest
t("8 >> 1.9") { 8 >> 1.9 }
t("Integer.__ensure(2**70)") { Integer.__ensure(2**70) }
t("Integer.__ensure(1.9)") { Integer.__ensure(1.9) }
t("[].shuffle(random: 1)") { [].shuffle(random: 1) }
t("[1].shuffle(1)") { [1].shuffle(1) }
t("[1, 2].shuffle(random: 1)") { [1, 2].shuffle(random: 1) }
t("setbyte 321") { "ab".setbyte(0, 321) }
t("split limit 1") { s = "a b"; r = s.split(" ", 1); [r, r[0].equal?(s)] }
t("split '' limit 1") { "".split(" ", 1) }
t("split nil limit 1") { "a b".split(nil, 1) }
[0, 1, -1, 37].each do |b|
  ["11", "0x1f", "0b11", "0o17", "017", "  -12z"].each do |s|
    t("#{s.inspect}.to_i(#{b})") { s.to_i(b) }
  end
end
t("[1, 2] * ','") { [1, 2] * "," }
