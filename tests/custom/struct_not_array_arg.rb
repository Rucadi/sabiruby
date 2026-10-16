# The Array methods read an argument as an Array only when it is one in the reference's sense
# (docs/plans/rc2-leftovers6-plan.md, S1). Expectation: mruby 4.1.0-rc2 (.rc.out).
# `mrb_array_p` (include/mruby/value.h) is MRB_TT_ARRAY, so a Struct or a Data (MRB_TT_STRUCT)
# is not an Array: `mrb_get_args("A"/"a")` raise through `mrb_ensure_array_type` ("X cannot be
# converted to Array", src/object.c), `mrb_check_array_type` answers nil (and sends no
# `to_ary`), `ary_eq`/`mrb_ary_cmp` answer false/nil, flatten, join, splice and puts keep the
# value whole, and a splat, `case ... when *v` and `Array()` go through `to_a`.
S = Struct.new(:x, :y)
D = Data.define(:x, :y)
class SubA < Array; end
class TA; def to_ary; [1, 2]; end; def to_a; [1, 2]; end; def inspect; "TA"; end; def to_s; "TA"; end; end
class B; def to_a; 1; end; end
class C; def to_a; S.new(1, 2); end; end
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end
[["s", S.new(1, 2)], ["d", D.new(x: 1, y: 2)], ["sub", SubA[1, 2]], ["int", 1], ["ta", TA.new]].each do |n, v|
  t("#{n} +") { [1] + v }
  t("#{n} concat") { [1, 2].concat(v) }
  t("#{n} replace") { [3, 4].replace(v) }
  t("#{n} initialize_copy") { [3, 4].send(:initialize_copy, v) }
  t("#{n} product") { [3, 4].product(v) }
  t("#{n} product blk") { r = []; [3].product(v) { |x| r << x }; r }
  t("#{n} flatten") { [v].flatten }
  t("#{n} flatten!") { a = [[v]]; a.flatten!(1); a }
  t("#{n} ==") { [1, 2] == v }
  t("#{n} eql?") { [1, 2].eql?(v) }
  t("#{n} <=>") { [3, 4] <=> v }
  t("#{n} -") { [1, 3] - v }
  t("#{n} |") { [3] | v }
  t("#{n} &") { [1, 3] & v }
  t("#{n} union") { [3].union(v) }
  t("#{n} difference") { [1, 3].difference(v) }
  t("#{n} intersection") { [1, 3].intersection(v) }
  t("#{n} intersect?") { [1, 3].intersect?(v) }
  t("#{n} assoc") { [v].assoc(1) }
  t("#{n} rassoc") { [v].rassoc(2) }
  t("#{n} join") { [v, 3].join("-") }
  t("#{n} []= range") { a = [7, 8, 9]; a[0..1] = v; a }
  t("#{n} []= len") { a = [7, 8, 9]; a[0, 2] = v; a }
  t("#{n} Array.new") { Array.new(v) }
  t("#{n} Array.new2") { Array.new(v, 0) }
  t("#{n} include?") { [[1, 2]].include?(v) }
  t("#{n} sum") { [v].sum([]) }
  t("#{n} Array()") { Array(v) }
  t("#{n} splat") { [*v] }
  t("#{n} case") { case 1; when *v then :hit; else :miss; end }
  t("#{n} Hash[]") { Hash[[v]] }
  t("#{n} Hash[] 1") { Hash[v] }
  t("#{n} set_backtrace") { e = RuntimeError.new("x"); e.set_backtrace(v); e.backtrace }
  t("#{n} puts") { puts v; puts [v]; :ok }
end
t("concat 0") { [1].concat }
t("concat 2") { [1].concat([2], [3]) }
t("Array.new 2.5") { Array.new(2.5) }
t("Array.new nil") { Array.new(nil) }
t("Array.new ary blk") { Array.new([1, 2]) { 0 } }
t("Array.new 2 blk") { Array.new(2) { |i| i * 3 } }
t("Array.new") { Array.new }
t("Array.new ary") { a = [1, 2]; b = Array.new(a); [b, b.equal?(a)] }
t("set_backtrace strings") { e = RuntimeError.new("x"); e.set_backtrace(["a", "b"]); e.backtrace }
t("set_backtrace nil") { e = RuntimeError.new("x"); e.set_backtrace(nil); e.backtrace }
t("product nil") { [1].product(nil) }
t("product 1.5") { [1].product(1.5) }
t("product sym") { [1].product(:a) }
t("product true") { [1].product(true) }
t("product ok") { [1].product([2], [3]) }
t("B splat") { [*B.new] }
t("C splat") { [*C.new] }
t("B case") { case 1; when *B.new then 1; end }
t("C case") { case 1; when *C.new then 1; end }
t("B Array()") { Array(B.new).size }
t("C Array()") { Array(C.new).size }
t("C args") { def f(*a) = a; f(*C.new) }
t("Hash() struct") { Hash(S.new(nil, nil)) }
t("Hash() []") { Hash([]) }
s = S.new(1, 2)
t("flatten nested") { [[1, [s, [2]]]].flatten }
t("join nested") { [[1, [s]], 2].join(",") }
t("puts nested") { puts [1, [s]]; 1 }
t("Struct to_a") { s.to_a == [1, 2] }
t("Struct ==") { s == S.new(1, 2) }
t("Struct each") { r = []; s.each { |x| r << x }; r }
t("Struct values") { s.values }
t("Struct deconstruct") { case s; in [a, b] then [a, b]; end }
t("Struct dig") { S.new([1, [2]], 0).dig(:x, 1, 0) }
t("Struct values_at") { s.values_at(0, 1) }
t("Struct to_h") { s.to_h }
t("Data to_h") { D.new(x: 1, y: 2).to_h }
t("Data ==") { D.new(x: 1, y: 2) == D.new(x: 1, y: 2) }
t("Data with") { D.new(x: 1, y: 2).with(y: 5) }
t("masgn splat") { a, b = *s; [a, b] }
