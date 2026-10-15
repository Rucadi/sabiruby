# Where a `def`, `alias` and `undef` add, where a constant and a class variable are read from,
# and what visibility a method gets, in methods with a receiver, in class bodies written as
# blocks and in `class << self`. Expectation: mruby 4.1.0-rc2 (.rc.out), which SabiRuby keeps
# (docs/worklog/2026-10-16-follow-master.md). mruby 891d1d9d answers most of these differently:
# its cref model (`mrb_vm_cref_class`, `mrb_vm_definee_class`, `MRB_PROC_GIVEN`,
# `caller_scope_visibility`, `method_defined?` with inherit false) came after the stable branch
# point. Each step is wrapped so that one error does not hide the rest; an error prints its
# class only (the two spell a singleton class differently in a NameError's message).
# .expected is SabiRuby's answer, which differs from rc2's (.rc.out) on four lines that were
# SabiRuby's own before any of this (found 2026-10-16, not changed here): a constant read in a
# block run by `class_eval` is looked up from the class given (rc2: the block's lexical scope);
# `attr_*` under `private` defines private methods, and one called on another class from a
# `private` scope defines public ones (rc2: public in both); `method_defined?` takes a second
# argument (rc2: ArgumentError) and leaves out a private method (rc2: true).
def try
  yield
rescue Exception => e
  e.class
end
class DefInSdef
  def orig; :orig; end
  def self.go; def m; :m; end; end
  def self.ali; alias aliased orig; end
  def self.und; undef orig; end
end
p [try { DefInSdef.go }, try { DefInSdef.ali }]
p [try { DefInSdef.new.m }, DefInSdef.respond_to?(:m), try { DefInSdef.new.aliased }]
p try { DefInSdef.und }
p DefInSdef.new.respond_to?(:orig)

k = Class.new { def self.go; def m; :class_new; end; end }
p try { k.go }
p [try { k.new.m }, k.respond_to?(:m)]
c = Class.new { def pos; def q; :q; end; end }
p try { c.new.pos }
p [c.method_defined?(:q), Object.method_defined?(:q)]

K = :top
class ConstBase; K = :base; end
class ConstSub < ConstBase
  def self.plain; K; end
  class << self
    def in_sclass; K; end
  end
end
p [try { ConstSub.plain }, try { ConstSub.in_sclass }]

module Lex
  K2 = :lex
  @@cv = :lex
  def self.make
    Class.new { def k; K2; end; def cv; @@cv; end }
  end
  def self.under(m); m.class_eval { K2 }; end
end
module Recv; K2 = :recv; end
o = Lex.make.new
p [try { o.k }, try { o.cv }, try { Lex.under(Recv) }]
try { Module.new.class_eval { [1].each { DefinedInBlock = 1 } } }
p Object.const_defined?(:DefinedInBlock, false)

v = Class.new {
  private
  attr_reader :r
  define_method(:d) { 1 }
  class << self
    private
    def hidden; end
  end
}
p [v.private_instance_methods(false).sort, v.respond_to?(:hidden), v.respond_to?(:hidden, true)]
other = Class.new
Class.new { private; other.attr_accessor :pub }
p other.public_instance_methods(false)

mod = Module.new { def mpub; end }
cls = Class.new { include mod; def pub; end; private def priv; end }
p [try { cls.method_defined?(:pub, false) }, cls.method_defined?(:mpub), cls.method_defined?(:priv)]

module ModfuncDef
  module_function
  def go; def m; :modfunc; end; end
end
p try { ModfuncDef.go }
p [try { Class.new { include ModfuncDef }.new.m }, ModfuncDef.respond_to?(:m)]
