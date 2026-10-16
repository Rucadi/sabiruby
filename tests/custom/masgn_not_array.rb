# A multiple assignment and a block's parameters do not call `to_ary`, and a Struct or a Data
# is not an Array to them (docs/plans/rc2-leftovers5-plan.md, R1). Expectation: mruby
# 4.1.0-rc2 (.rc.out). OP_AREF (src/vm.c) gives the value itself at index 0 and nil after it
# when `mrb_array_p` is false; OP_APOST wraps such a value in a one-element Array; OP_ENTER
# spreads a block's single argument only when it is `mrb_array_p` (MRB_TT_ARRAY, so not a
# Struct, MRB_TT_STRUCT). An explicit splat (`*obj`) still calls `to_a`.
S = Struct.new(:x, :y)
D = Data.define(:x, :y)
class MyAry; def to_ary; [1, 2, 3]; end; def inspect; "MyAry"; end; end
class Bad; def to_ary; 5; end; def inspect; "Bad"; end; end
o = MyAry.new
a, b, c = o; p [a, b, c]
a, *b = o; p [a, b]
*a, b = o; p [a, b]
a, (b, c) = 1, o; p [a, b, c]
a, (b, c) = [1, o]; p [a, b, c]
a, b = 1, o; p [a, b]
x = (a, b = o); p x
x = S.new([7, 8], 0); a, b = x.x; p [a, b]
[o].each { |a, b| p [a, b] }
[o].each { |a, *b| p [a, b] }
[o].each { |a| p a }
[[o, 2]].each { |(a, b), c| p [a, b, c] }
pr = proc { |a, b| p [a, b] }; pr.call(o)
la = lambda { |a, b| p [a, b] } ; begin; la.call(o); rescue => e; p e; end
def m((a, b)); p [a, b]; end; m(o)
a, b = Bad.new; p [a, b]
[Bad.new].each { |a, b| p [a, b] }
a, b = nil; p [a, b]
a, b = 5; p [a, b]
a, b = *o; p [a, b]
a, b = [o]; p [a, b]
h = {o => 1}; h.each { |a, b| p [a, b] }
def yy; yield MyAry.new; end
yy { |a, b| p [a, b] }
yy { |a, (b, c)| p [a, b, c] }

s = S.new(1, 2)
a, b = s; p [a, b]
a, *b = s; p [a, b]
*a, b = s; p [a, b]
[s].each { |a, b| p [a, b] }
d = D.new(x: 1, y: 2)
a, b = d; p [a, b]
[d].each { |a, b| p [a, b] }
class SubA < Array; end
sa = SubA[1, 2]
a, b = sa; p [a, b]
[sa].each { |a, b| p [a, b] }
a, b = Set[1, 2]; p [a, b]
a, b = {1 => 2}; p [a, b]
[{1=>2}].each { |a, b| p [a, b] }
a, b = (1..2); p [a, b]
a, b = "ab"; p [a, b]

class T < S; end
t = T.new(3, 4)
a, b = s.dup; p [a, b]
a, b = t; p [a, b]
[t].each { |a, b| p [a, b] }
proc { |a, b| p [a, b] }.call(s)
def y1; yield S.new(5, 6); end
y1 { |a, (b, c)| p [a, b, c] }
y1 { |*a| p a }
a, b = *s; p [a, b]
p [*s]
p s.to_a
a, (b, c) = 1, s; p [a, b, c]
a, b = D.new(x: 1, y: 2).dup; p [a, b]
a, b = S.new(1, 2).freeze; p [a, b]
