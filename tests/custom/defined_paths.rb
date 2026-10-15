# `defined?` on a constant path of any depth and on a method reached through a receiver, as the
# compiler of mruby 891d1d9d asks them: `__defined_const_path?(root, [names])` and
# `__defined_method_on?(recv, name)` (src/kernel.c there). This case is compiled by rc2's `mrbc`
# (tools/custom.sh), which emits neither form, so it makes the calls that compiler makes; the
# rc2 form `__defined_const_path?(:A, :B)` is kept for the compiler `eval` uses, and is
# reached through `eval`. Also: keywords of a method with exactly 15 positional parameters (`enter_kwpos`).
# docs/plans/edge-diff-fix-plan.md D3. Expectation: the output of mruby 891d1d9d (Docker
# kishima/mruby:891d1d9d); rc2 has neither helper (.rc.out).
module Outer
  module Mid; Leaf = 1; end
  NotAModule = 1
  def self.lexical; __defined_const_path?(nil, [:Mid, :Leaf]); end
end
p __defined_const_path?(nil, [:Outer, :Mid, :Leaf])
p __defined_const_path?(nil, [:Outer, :Missing])
p __defined_const_path?(nil, [:NoSuch, :Mid])
p __defined_const_path?(nil, [:Outer, :NotAModule, :Leaf])
p __defined_const_path?(Object, [:Outer, :Mid])
p __defined_const_path?(Object, [:String])
p __defined_const_path?(String, [:String])
p __defined_const_path?(1, [:Leaf])
p Outer.lexical
# the compiler `eval` uses here is rc2's, which asks in the rc2 form
p eval("defined?(Outer::Mid)"), eval("defined?(Outer::Nope)")

class Recv
  def pub; end
  private def priv; end
  protected def prot; end
  def respond_to_missing?(name, priv = false) = name == :ghost
  def inside(o) = __defined_method_on?(o, :prot)
end
o = Recv.new
p [:pub, :priv, :prot, :ghost, :none].map { |m| __defined_method_on?(o, m) }
p o.inside(Recv.new)
p __defined_method_on?(1, :+)

a15 = (1..15).to_a
def m15(p1,p2,p3,p4,p5,p6,p7,p8,p9,p10,p11,p12,p13,p14,p15, k: 1) [p15, k] end
p m15(*a15, k: 7), m15(*a15)
p(begin; m15(*a15, z: 9); rescue ArgumentError => e; e.message; end)
def m15r(p1,p2,p3,p4,p5,p6,p7,p8,p9,p10,p11,p12,p13,p14,p15, k:) k end
p m15r(*a15, k: 7)
l = ->(p1,p2,p3,p4,p5,p6,p7,p8,p9,p10,p11,p12,p13,p14,p15, k: 1) { [p15, k] }
p l.call(*a15, k: 7)
