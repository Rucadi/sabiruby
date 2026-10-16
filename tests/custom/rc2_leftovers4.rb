# The small differences from mruby 4.1.0-rc2 left after rc2-leftovers3 (docs/plans/
# rc2-leftovers4-plan.md, P2). Expectation: mruby 4.1.0-rc2 (.rc.out).
# `mrb_frozen_error` (src/error.c) says "can't modify frozen %T": the class, without the
# object's inspect. A method table write to the singleton class of a frozen object
# (`mt_writable`, src/class.c) checks the attached object, so it names the object's class;
# `extend` (`mrb_include_module`) checks the singleton class itself and names Class.
# `mrb_str_aset` (src/string.c) says "string not matched" for a String not found and for a
# Range out of the string (both STR_OUT_OF_RANGE). `mrb_ary_delete_at` (src/array.c) reads its
# index with `mrb_get_args("i")`, `mrb_as_int`; `MatchData#[]` is rc2_leftovers4_regexp.rb.
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue => e
  puts "#{label} => #{e.class}: #{e.message}"
end
a = [1].freeze
t("slice!") { a.slice!(0) }
t("insert") { a.insert(0, 1) }
t("compact!") { a.compact! }
h = {a: 1}.freeze
t("hash delete") { h.delete(:a) }
t("hash rehash") { h.rehash }
o = Object.new.freeze
t("ivar") { o.instance_variable_set(:@a, 1) }
t("def o.x") { def o.x; end }
t("define_method") { o.singleton_class.define_method(:y) {} }
t("extend") { o.extend(Comparable) }
c = Class.new.freeze
t("class const_set") { c.const_set(:X, 1) }
t("str not matched") { s = "abcde"; s["z"] = "X"; s }
t("range out") { s = "abcde"; s[9..10] = "X"; s }
t("index out") { s = "abcde"; s[9] = "X"; s }
[nil, "1", Float::NAN, 1.0 / 0, 2**70, 1.5].each do |x|
  t("delete_at(#{x.inspect})") { [1, 2].delete_at(x) }
end
