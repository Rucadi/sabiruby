# OP_EQ answered where it stands (rc2-sweep-fixes Y8c), as rc2's OP_EQ: two values that are
# the same object are equal (mrb_obj_eq) unless the value is a NaN, a Symbol receiver is
# equal to nothing else, and only then the numbers and the `==` send. Every operand goes
# through `v` so that the compiler cannot fold the comparison away. Expectation: the
# reference mruby's own output.
def v(x) = x
def show(x)
  r = begin; yield; rescue => e; "#{e.class}: #{e.message}"; end
  puts "#{x}: #{r.inspect}"
end

# numbers
show("1==1") { v(1) == v(1) }
show("1==2") { v(1) == v(2) }
show("1==1.0") { v(1) == v(1.0) }
show("1.0==1") { v(1.0) == v(1) }
show("0.0==-0.0") { v(0.0) == v(-0.0) }
show("-0.0==0") { v(-0.0) == v(0) }
show("2**53+1==(2**53+1).to_f") { v(2**53 + 1) == v((2**53 + 1).to_f) }
show("2**70==2**70") { v(2**70) == v(2**70) }
x = v(2**70)
show("x==x (wide)") { x == x }
show("2**70==(2**70).to_f") { v(2**70) == v((2**70).to_f) }
# NaN: equal to nothing, itself included
n = v(Float::NAN)
show("n==n") { n == n }
show("NAN==NAN") { v(Float::NAN) == v(Float::NAN) }
show("n==1") { n == v(1) }
show("1==n") { v(1) == n }
show("n.equal?(n)") { n.equal?(n) }
show("n!=n") { n != n }
show("inf==inf") { v(Float::INFINITY) == v(Float::INFINITY) }
# (two Arrays of a NaN made apart differ: README "Known deviations", a NaN has no identity)
an = v([Float::NAN])
show("[n]==[n] same") { an == an }
# Symbols
show(":a==:a") { v(:a) == v(:a) }
show(":a==:b") { v(:a) == v(:b) }
show(":a=='a'") { v(:a) == v("a") }
show("'a'==:a") { v("a") == v(:a) }
show(":a==nil") { v(:a) == v(nil) }
show(":a==1") { v(:a) == v(1) }
show("nil==:a") { v(nil) == v(:a) }
# the immediates
show("nil==nil") { v(nil) == v(nil) }
show("nil==false") { v(nil) == v(false) }
show("true==true") { v(true) == v(true) }
show("false==false") { v(false) == v(false) }
show("true==1") { v(true) == v(1) }
# objects
s = v("abc")
show("s==s") { s == s }
show("'abc'=='abc'") { v("abc") == v("abc") }
show("'abc'=='abd'") { v("abc") == v("abd") }
h = v({a: 1})
show("h==h") { h == h }
o = v(Object.new)
show("o==o") { o == o }
show("o==Object.new") { o == v(Object.new) }
# a class whose == says no to everything, itself included: the same object is equal anyway
class Never
  def ==(o) = false
end
nv = v(Never.new)
show("nv==nv") { nv == nv }
show("nv==Never.new") { nv == v(Never.new) }
show("nv.==(nv)") { nv.==(nv) }
show("nv.send(:==, nv)") { nv.send(:==, nv) }
show("nv!=nv") { nv != nv }
# == that answers something other than true or false
class Odd
  def ==(o) = "odd #{o.class}"
end
od = v(Odd.new)
show("od==od") { od == od }
show("od==1") { od == v(1) }
show("od==:a") { od == v(:a) }
# == that raises
class Raises
  def ==(o) = raise("eq called")
end
ra = v(Raises.new)
show("ra==ra") { ra == ra }
show("ra==1") { ra == v(1) }
# a String subclass with its own ==
class S2 < String
  def ==(o) = "S2=="
end
s2 = v(S2.new("x"))
show("s2==s2") { s2 == s2 }
show("s2=='x'") { s2 == v("x") }
show("'x'==s2") { v("x") == s2 }
# redefining Symbol#== and Integer#==: rc2's OP_EQ answers a Symbol receiver without the
# send, and two Integers as numbers
class Symbol
  alias_method :orig_eq, :==
  def ==(o) = "sym redefined"
end
show(":a==:a redefined") { v(:a) == v(:a) }
show(":a==:b redefined") { v(:a) == v(:b) }
show(":a.==(:b) redefined") { v(:a).==(v(:b)) }
class Symbol
  alias_method :==, :orig_eq
end
class Integer
  alias_method :orig_eq, :==
  def ==(o) = "int redefined"
end
show("1==1 redefined") { v(1) == v(1) }
show("1==2 redefined") { v(1) == v(2) }
show("1==1.0 redefined") { v(1) == v(1.0) }
show("x==x redefined (wide)") { x == x }
show("2**70==2**70 redefined") { v(2**70) == v(2**70) }
class Integer
  alias_method :==, :orig_eq
end
# a loop through the same instruction
c = 0
i = v(0)
syms = [:a, :b, :c]
while i < v(300)
  c += 1 if syms[i % 3] == v(:b)
  c += 1 if nv == nv
  c += 1 if i == v(150)
  i += 1
end
show("loop") { c }
