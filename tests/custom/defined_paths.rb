# `defined?` on constant paths and on methods reached through a receiver, as rc2's `mrbc` asks
# them (`__defined_const_path?(:A, :B)` for a two-name path, nothing for a longer one or one
# rooted at `::`, and nothing for `recv.meth`), and the keywords of a method with exactly 15
# positional parameters. Expectation: mruby 4.1.0-rc2 (.rc.out), which SabiRuby keeps
# (docs/worklog/2026-10-16-follow-master.md). mruby 891d1d9d answers several differently: its
# compiler asks every path and receiver (`__defined_const_path?(root, [names])`,
# `__defined_method_on?`, 97ef20933, 3c6c4d53a) and its VM delivers the 15th parameter's
# keywords (9f3a1ccb9), all after the stable branch point.
module Outer
  module Mid; Leaf = 1; end
  NotAModule = 1
  def self.lexical; defined?(Mid::Leaf); end
end
p defined?(Outer::Mid::Leaf)
p defined?(Outer::Mid)
p defined?(Outer::Missing)
p defined?(NoSuch::Mid)
p defined?(::Outer::Mid)
p defined?(String::String)
p Outer.lexical
p eval("defined?(Outer::Mid)"), eval("defined?(Outer::Nope)")

class Recv
  def pub; end
  private def priv; end
  def respond_to_missing?(name, priv = false) = name == :ghost
end
o = Recv.new
p [defined?(o.pub), defined?(o.priv), defined?(o.ghost), defined?(o.none)]
p [defined?(puts), defined?(String), defined?(@iv)]

a15 = (1..15).to_a
def m15(p1,p2,p3,p4,p5,p6,p7,p8,p9,p10,p11,p12,p13,p14,p15, k: 1) [p15, k] end
p m15(*a15, k: 7), m15(*a15)
