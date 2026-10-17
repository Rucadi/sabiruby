# The differences from mruby 4.1.0-rc2 found in rc2-leftovers8 (docs/plans/
# rc2-leftovers9-plan.md, V1). Expectation: mruby 4.1.0-rc2 (.rc.out).
# The four number classes have only the methods rc2 has (`numeric_rom_entries`,
# `integer_rom_entries`, `float_rom_entries` in src/numeric.c, and mrblib/numeric.rb): no
# `Float#to_int`, `Integer#pred`, `Numeric#nan?` (`nan?` is Float's own, `flo_nan_p`),
# `Numeric#step`, `Numeric#to_int`, `Integer#__num_to_a` (Range's) or
# `Float#__coerce_step_counter` (Integer's). A method written in Ruby that is given the wrong
# number of arguments states only the required count, m1+m2, even with optional or rest
# parameters (`argnum_error(mrb, m1+m2)` of `OP_ENTER`, src/vm.c).
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end

# methods rc2 does not have
t("1.0.to_int") { 1.0.to_int }
t("1.pred") { 1.pred }
t("1.nan?") { 1.nan? }
t("(2**70).nan?") { (2**70).nan? }
t("Rational(1, 2).nan?") { Rational(1, 2).nan? }
t("Rational(1, 2).to_int") { Rational(1, 2).to_int }
t("Rational(1, 2).step") { Rational(1, 2).step(2) { } }
t("1.__num_to_a") { 1.__num_to_a }
t("1.0.__coerce_step_counter(1)") { 1.0.__coerce_step_counter(1) }
t("respond_to? to_int") { [1.respond_to?(:to_int), 1.0.respond_to?(:to_int), Rational(1, 2).respond_to?(:to_int)] }
t("respond_to? nan?") { [1.respond_to?(:nan?), 1.0.respond_to?(:nan?)] }
t("Numeric ims") { [:nan?, :step, :to_int].map { |m| Numeric.method_defined?(m) } }
t("Float#nan? owner") { Float.instance_method(:nan?).owner }
t("Integer#to_int owner") { Integer.instance_method(:to_int).owner }
# what still answers
t("1.0.nan?") { 1.0.nan? }
t("(0.0 / 0.0).nan?") { (0.0 / 0.0).nan? }
t("1.0.nan?(1)") { 1.0.nan?(1) }
t("1.to_int") { 1.to_int }
t("1.__coerce_step_counter(1.0)") { 1.__coerce_step_counter(1.0) }
t("1.step(2, 0.5)") { 1.step(2, 0.5).to_a }
t("1.0.step(2.0, 0.5)") { 1.0.step(2.0, 0.5).to_a }
t("(1..3).to_a") { (1..3).to_a }
t("[1, 2][1.0]") { [1, 2][1.0] }

# the wrong number of arguments of a method written in Ruby
def f1(a); end
def f2(a, b = nil); end
def f3(a, b = nil, *r); end
def f4(a, b = nil, c); end
def f5(a, k: 1); end
def f7(*r, a); end
def f8(a, **kw); end
def f10(a, b, c = 1, d = 2, e); end
t("f1()") { f1 }
t("f1(1, 2)") { f1(1, 2) }
t("f2()") { f2 }
t("f2(1, 2, 3)") { f2(1, 2, 3) }
t("f2(1, 2, k: 3)") { f2(1, 2, k: 3) }
t("f3()") { f3 }
t("f4(1)") { f4(1) }
t("f4(1, 2, 3, 4)") { f4(1, 2, 3, 4) }
t("f5()") { f5 }
t("f5(1, 2)") { f5(1, 2) }
t("f5(1, 2, k: 2)") { f5(1, 2, k: 2) }
t("f7()") { f7 }
t("f8()") { f8 }
t("f8(1, 2)") { f8(1, 2) }
t("f10(1)") { f10(1) }
t("f10(1, 2, 3, 4, 5, 6)") { f10(1, 2, 3, 4, 5, 6) }
t("f2(*Array.new(20, 1))") { f2(*Array.new(20, 1)) }
t("lambda") { ->(a, b = 1) { }.call(1, 2, 3) }
t("lambda0") { ->(a, b = 1) { }.call }
t("proc") { proc { |a, b = 1| [a, b] }.call(1, 2, 3) }
t("yield to a lambda") { def yl; yield 1, 2, 3; end; yl(&->(a, b = 1) { }) }
t("1.clamp(1, 2, 3)") { 1.clamp(1, 2, 3) }
