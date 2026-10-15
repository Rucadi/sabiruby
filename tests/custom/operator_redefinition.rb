# An operator redefined on Integer, Float, Symbol and nil/true/false, as mruby 4.1.0-rc2 answers
# it: the arithmetic and comparison opcodes answer a pair of numbers themselves, `OP_EQ` answers
# the same object true and a Symbol receiver false without a send, and `mrb_equal` (`Array#index`
# here) sends every `==` that is not the plain one. mruby 891d1d9d sends from the opcodes while
# an operator is redefined (`bop_redefined`, d8873e7b1, 35b0428c9, fa3dead08) and asks a class's
# own `==` about the receiver itself (55ae46dcf), after the stable branch point. Expectation:
# mruby 4.1.0-rc2 (.rc.out), which SabiRuby keeps (docs/worklog/2026-10-16-follow-master.md).
def show(*a) = puts(a.inspect)

class Integer
  alias_method :__add, :+; alias_method :__lt, :<; alias_method :__eq, :==; alias_method :__div, :/
  def +(o) [:add, self, o] end
  def <(o) [:lt, self, o] end
  def ==(o) [:eq, self, o] end
  def /(o) [:div, self, o] end
end
a = 7; b = 2
r = [a + b, a + 1, a < b, a == a, a / b, [a].index(b)]
lv = a; lv += 1; r << lv
f = 1.5 + a
class Integer
  alias_method :+, :__add; alias_method :<, :__lt; alias_method :==, :__eq; alias_method :/, :__div
end
show(*r, f)
show(a + b, a < b, a == a, a / b, [a].index(b))

class Float
  alias_method :__mul, :*; alias_method :__ge, :>=
  def *(o) [:mul, self, o] end
  def >=(o) [:ge, self, o] end
end
x = 2.5
r = [x * 2, x * 2.0, 2 * x, x >= 1, 1 >= x, 3 >= 2]
fl = x; fl *= 3; r << fl
class Float
  alias_method :*, :__mul; alias_method :>=, :__ge
end
show(*r)
show(x * 2, x >= 1)

class Symbol
  alias_method :__eq, :==
  def ==(o) [:sym, self, o] end
end
s = :a
r = [s == s, s == :b, [:a, :b].index(:b)]
class Symbol; alias_method :==, :__eq; end
show(*r)
show(s == s, s == :b)

own = Class.new { def ==(o) :own end }.new
show(own == own, own != own)
cmp = Class.new { include Comparable; def <=>(o) nil end }.new
show(cmp == cmp)
plain = Object.new
show(plain == plain)

# last: mruby keeps the mark of nil, true and false once it is made (a class flag), so
# after this `OP_EQ` asks every receiver's `==`, Comparable's among them; SabiRuby rechecks
# what `==` resolves to (docs/worklog/2026-10-15-edge-diff-fix.md, D1)
class NilClass
  alias_method :__eq, :==
  def ==(o) :nil_eq end
end
n = nil
r = [n == n, true == true, 1 == 1]
class NilClass; alias_method :==, :__eq; end
show(*r, n == n)

