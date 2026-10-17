# The natives whose argument conversion rc2-leftovers7 changed (`Vm::expect_int` and
# `expect_str` now build their errors in cold functions; `sort` without a block, `values_at`,
# `Float#floor`, `Integer#<<`, `Array#*`, `split`, `String#to_i`), each called with arguments
# that convert, so only the arms that answer are on the clock.
class P; def initialize(v); @v = v; end; attr_reader :v; def <=>(o); @v <=> o.v; end; end
a = [5, 3, 8, 1]
objs = [P.new(3), P.new(1), P.new(2)]
s = "a b c"
n = 0
i = 0
while i < 50_000
  n += a.first(2).size
  n += a.at(1)
  n += a.rotate(1)[0]
  n += a.values_at(0, 2)[0]
  n += "ab".getbyte(0)
  n += 1.5.floor + 2.5.ceil
  n += 1 << 3
  n += (a * 2).size
  n += a.sort[0]
  n += objs.sort[0].v
  n += s.split(" ").size
  n += "11".to_i + "ff".to_i(16)
  n += ("x" * 3).size
  i += 1
end
