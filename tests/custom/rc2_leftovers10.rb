# rc2-leftovers10 (docs/plans/rc2-leftovers10-plan.md): what 4.1.0-rc2 answers for
#   W1: the argument spec of a native (`MRB_ARGS_*`): `Method#arity`, and the count checked
#       before the native runs (src/vm.c `check_argument_count`)
#   W3: a `define_method` block runs as a lambda (`define_method_m` sets `MRB_PROC_STRICT`)
#   W4: `new` that a class's singleton undefines (`mrb_undef_class_method`) is a NoMethodError
#       "for Class", the class of the receiver
# expected-from: the reference (.expected is .rc.out).
# regexp-only: String#index, #sub and #slice! are mruby-regexp's in the reference's build, and
# their specs and arities differ from the core methods a build without the gem has.
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end

# W1: arity from the declaration
t("1.method(:+).arity") { 1.method(:+).arity }
t("'a'.method(:upcase).arity") { "a".method(:upcase).arity }
t("'a'.method(:index).arity") { "a".method(:index).arity }
t("[].method(:push).arity") { [].method(:push).arity }
t("[].method(:first).arity") { [].method(:first).arity }
t("{}.method(:fetch).arity") { {}.method(:fetch).arity }
t("1.5.method(:to_i).arity") { 1.5.method(:to_i).arity }
t("Kernel.method(:sprintf).arity") { Kernel.method(:sprintf).arity }
t("Math.method(:atan2).arity") { Math.method(:atan2).arity }
t("String.instance_method(:sub).arity") { String.instance_method(:sub).arity }
t("Object.instance_method(:send).arity") { Object.instance_method(:send).arity }
t("Module.instance_method(:define_method).arity") { Module.instance_method(:define_method).arity }
t("Struct.new(:a).instance_method(:a).arity") { Struct.new(:a).instance_method(:a).arity }
t("Data.define(:a).instance_method(:a).arity") { Data.define(:a).instance_method(:a).arity }
t("Data.define(:a,:b,:c,:d,:e).instance_method(:e).arity") { Data.define(:a, :b, :c, :d, :e).instance_method(:e).arity }
class AR; attr_reader :r; attr_writer :w; end
t("attr_reader arity") { AR.instance_method(:r).arity }
t("attr_writer arity") { AR.instance_method(:w=).arity }
# W1: the count, checked before the body
t("1.+(1, 2)") { 1.+(1, 2) }
t("'a'.upcase(1)") { "a".upcase(1) }
t("'abc'.index") { "abc".index }
t("'abc'.index('a', 0, 1)") { "abc".index("a", 0, 1) }
t("[].first(1, 2)") { [].first(1, 2) }
t("{}.fetch") { {}.fetch }
t("Math.atan2(1)") { Math.atan2(1) }
t("1.send(:+)") { 1.send(:+) }
t("1.method(:+).call") { 1.method(:+).call }
t("1.public_send(:+, 1, 2)") { 1.public_send(:+, 1, 2) }
t("nil.to_s(1)") { nil.to_s(1) }
t("'a'.upcase(k: 1)") { "a".upcase(k: 1) }
t("'a'.upcase(**{})") { "a".upcase(**{}) }
t("Object.new(1)") { Object.new(1) }
t("Object.new.define_singleton_method(:x, proc {})") { Object.new.define_singleton_method(:x, proc {}) }
t("frozen 'a'.slice!") { "a".freeze.slice! }
t("frozen 'a'.slice!(0)") { "a".freeze.slice!(0) }
t("Comparable#between?") { 1.between?(1) }
t("1 <=> (1, 2)") { 1.send(:<=>, 1, 2) }

# W3: define_method's block is strict
class C
  define_method(:g) { |a, b = 1| [a, b] }
  define_method(:h) { |a| a }
  define_method(:z) { 0 }
end
t("C#g(1)") { C.new.g(1) }
t("C#g(1, 2)") { C.new.g(1, 2) }
t("C#g(1, 2, 3)") { C.new.g(1, 2, 3) }
t("C#g()") { C.new.g }
t("C#h([1, 2])") { C.new.h([1, 2]) }
t("C#h(1, 2)") { C.new.h(1, 2) }
t("C#z(1)") { C.new.z(1) }
t("C#g arity") { C.instance_method(:g).arity }
t("C#h arity") { C.instance_method(:h).arity }
o = Object.new
o.define_singleton_method(:s) { |a, b = 1| [a, b] }
t("o.s(1, 2, 3)") { o.s(1, 2, 3) }
t("o.s arity") { o.method(:s).arity }
pr = proc { |a, b| [a, b] }
C.define_method(:k, &pr)
t("C#k(1)") { C.new.k(1) }
t("the block stays a proc") { [pr.lambda?, pr.call(1)] }
t("C#k(1, 2) returns") { C.new.k(1, 2) }
class C; define_method(:r) { |x| return x * 2; :not }; end
t("return in define_method") { C.new.r(3) }

# W4: an undefined singleton `new`
[Integer, Float, Symbol, NilClass, TrueClass, FalseClass, Rational, Complex, Method, UnboundMethod, MatchData, Binding].each do |k|
  t("#{k}.new") { k.new }
end
t("MatchData.allocate") { MatchData.allocate }
t("Binding.allocate") { Binding.allocate }
t("Data.new") { Data.new }
t("Integer.respond_to?(:new)") { Integer.respond_to?(:new) }
t("Integer.foo") { Integer.foo }
