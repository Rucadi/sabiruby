# OP_DIV answered where it stands (rc2-sweep-fixes Y8b), as rc2's vm_op_div: two Integers
# by mrb_div_int_value (a floor division; 0 raises, MRB_INT_MIN / -1 is a wide Integer), and
# the three pairs with a Float by mrb_div_float. Anything else is sent `/`. Every operand
# goes through `v` so that the compiler cannot fold the division away. Expectation: the
# reference mruby's own output.
def v(x) = x
def show(x)
  r = begin; yield; rescue => e; "#{e.class}: #{e.message}"; end
  puts "#{x}: #{r.inspect}"
end

# Integer / Integer, every sign
show("7/2") { v(7) / v(2) }
show("-7/2") { v(-7) / v(2) }
show("7/-2") { v(7) / v(-2) }
show("-7/-2") { v(-7) / v(-2) }
show("-8/2") { v(-8) / v(2) }
show("8/-2") { v(8) / v(-2) }
show("0/5") { v(0) / v(5) }
show("0/-5") { v(0) / v(-5) }
show("1/3") { v(1) / v(3) }
show("-1/3") { v(-1) / v(3) }
# zero
show("7/0") { v(7) / v(0) }
show("0/0") { v(0) / v(0) }
show("-7/0") { v(-7) / v(0) }
# the ends of the range
min = v(-9223372036854775807) - v(1)
max = v(9223372036854775807)
show("min/-1") { min / v(-1) }
show("min/1") { min / v(1) }
show("min/2") { min / v(2) }
show("min/-2") { min / v(-2) }
show("max/-1") { max / v(-1) }
show("min/min") { min / min }
show("min/max") { min / max }
show("max/min") { max / min }
show("(min/-1).class") { (min / v(-1)).class }
# Float
show("7.0/2.0") { v(7.0) / v(2.0) }
show("7/2.0") { v(7) / v(2.0) }
show("7.0/2") { v(7.0) / v(2) }
show("-7.5/2") { v(-7.5) / v(2) }
show("1.0/0") { v(1.0) / v(0) }
show("-1.0/0") { v(-1.0) / v(0) }
show("0.0/0") { v(0.0) / v(0) }
show("1/0.0") { v(1) / v(0.0) }
show("-1/0.0") { v(-1) / v(0.0) }
show("1/-0.0") { v(1) / v(-0.0) }
show("0/0.0") { v(0) / v(0.0) }
show("1.0/-0.0") { v(1.0) / v(-0.0) }
show("-0.0/1") { v(-0.0) / v(1) }
show("0.0/-1") { v(0.0) / v(-1) }
show("nan/2") { v(Float::NAN) / v(2) }
show("inf/inf") { v(Float::INFINITY) / v(Float::INFINITY) }
show("inf/-3") { v(Float::INFINITY) / v(-3) }
show("1/inf") { v(1) / v(Float::INFINITY) }
show("min/0.5") { min / v(0.5) }
show("max/0.25") { max / v(0.25) }
show("1e308/1e-10") { v(1e308) / v(1e-10) }
# wide Integers and other receivers are sent `/`
show("2**70/3") { v(2**70) / v(3) }
show("2**70/-3") { v(2**70) / v(-3) }
show("7/2**70") { v(7) / v(2**70) }
show("-7/2**70") { v(-7) / v(2**70) }
show("2**70/0") { v(2**70) / v(0) }
show("2**70/0.5") { v(2**70) / v(0.5) }
show("7/r") { v(7) / v(Rational(2, 3)) }
show("7.0/r") { v(7.0) / v(Rational(2, 3)) }
show("7/c") { v(7) / v(Complex(1, 1)) }
show("r/2") { v(Rational(1, 3)) / v(2) }
show("7/nil") { v(7) / v(nil) }
show("7/'a'") { v(7) / v("a") }
show("nil/7") { v(nil) / v(7) }
class Q
  def initialize(x) = @x = x
  def /(o) = "Q(#{@x})/#{o.inspect}"
end
show("Q/2") { v(Q.new(1)) / v(2) }
show("Q/0") { v(Q.new(1)) / v(0) }
# a loop through the same instruction
s = 0
i = v(1)
while i < v(100)
  s += v(1000) / i + v(-1000) / i
  i += 1
end
show("loop") { s }
f = 0.0
i = v(1)
while i < v(10)
  f += v(1.0) / i
  i += 1
end
show("floop") { f }
# redefining Integer#/ and Float#/: rc2 answers the numbers without looking (OP_DIV), the
# method answers a call by name
class Integer
  alias_method :orig_div, :/
  def /(o) = "redefined"
end
class Float
  alias_method :orig_fdiv, :/
  def /(o) = "redefined f"
end
show("7/2 redefined") { v(7) / v(2) }
show("7.0/2 redefined") { v(7.0) / v(2) }
show("7/2.0 redefined") { v(7) / v(2.0) }
show("7/0 redefined") { v(7) / v(0) }
show("min/-1 redefined") { min / v(-1) }
show("7.send(:/, 2)") { v(7).send(:/, v(2)) }
show("2**70/3 redefined") { v(2**70) / v(3) }
show("7/r redefined") { v(7) / v(Rational(2, 3)) }
class Integer
  alias_method :/, :orig_div
end
class Float
  alias_method :/, :orig_fdiv
end
show("7/2 restored") { v(7) / v(2) }
