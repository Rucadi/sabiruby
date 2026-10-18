# `OP_ADDILV`/`OP_SUBILV` (`x += 1` and `x -= 1` on a local with a small immediate; 4.1's compiler
# fuses the `MOVE` away). SabiRuby answers an Integer and a Float where they stand, as mruby's
# `OP_MATHILV` (src/vm.c:4154-4197) does; an Integer result past 64 bits goes the slow way (a wide
# Integer), and any other receiver is sent `+`/`-` with the immediate. Neither side looks at a
# redefined `Integer#+` or `Float#+` there. docs/plans/rc2-sweep-fixes-plan.md Y7, worklog
# docs/worklog/2026-10-18-rc2-sweep-fixes.md, Y7.
# expected-from: the reference (.expected is .rc.out).
# Every starting value comes through `v`, so the compiler cannot fold `x = 1; x += 1` into a
# constant, and the `+=` that should raise is in a method, outside `begin` (where it would not be
# fused); `.dump` shows the instruction on each of them.
def v(x); x; end
def add1(x); x += 1; x; end

# the Integer range: past the largest and below the smallest, then on from the wide Integer
i = v(9223372036854775806)
i += 1
p i
i += 1
p i, i.class
i += 200
p i
i -= 201
p i
j = v(-9223372036854775807)
j -= 1
p j
j -= 1
p j
j += 1
p j
# crossing the range inside a loop (the same local, the same instruction each time)
k = v(9223372036854775805)
n = v(0)
while n < 4
  k += 1
  n += 1
end
p k, n
# 0 and the largest immediates
m = v(0)
m += 0
m -= 0
m += 255
m -= 255
m -= 255
p m

# Float, its infinities and NaN
f = v(1.5)
f += 1
p f
f -= 3
p f
f = v(Float::INFINITY)
f += 1
p f
f = v(-Float::INFINITY)
f -= 1
p f
f = v(Float::NAN)
f += 1
p f.nan?
f = v(0.1)
f += 2
p f

# a receiver that is no Integer or Float is sent the method with the immediate
class Counter
  attr_reader :log
  def initialize; @log = []; end
  def +(o); @log << [:+, o]; self; end
  def -(o); @log << [:-, o]; :minus_answer; end
end
c = Counter.new
c += 3
p c.class, c.log
c -= 7
p c
r = v(Rational(1, 3))
r += 1
p r
z = v(Complex(1, 2))
z -= 1
p z
s = v("a")
begin
  s = add1(s)
rescue => e
  p e.class
end
x = v(nil)
begin
  x = add1(x)
rescue => e
  p e.class
end
p x
# a redefined `+` on a String subclass is called (it is a send)
class MyStr < String
  def +(o); "MyStr+#{o}"; end
end
y = MyStr.new("q")
y += 4
p y

# a redefined Integer#+ / Float#- is not looked at by the fast path (as `x + 1` with OP_ADDI);
# a wide Integer is no immediate value and is sent the method
class Integer
  alias_method :orig_plus, :+
  def +(o); :redefined; end
end
a = v(1)
a += 1
w = v(2**64)
w += 1
class Integer
  alias_method :+, :orig_plus
end
p a, w
class Float
  alias_method :orig_minus, :-
  def -(o); :redefined; end
end
g = v(2.5)
g -= 1
class Float
  alias_method :-, :orig_minus
end
p g
