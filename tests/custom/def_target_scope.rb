# Where a `def`, `alias` and `undef` add, where a constant and a class variable are read from,
# and what visibility a method gets, in methods with a receiver, in class bodies written as
# blocks and in `class << self` (mruby 891d1d9d: the cref kept on a scope's proc
# `mrb_vm_cref_class`, the class a `def` adds to `mrb_vm_definee_class`, `MRB_PROC_GIVEN`,
# `caller_scope_visibility`, `method_defined?` with inherit false;
# docs/plans/edge-diff-fix-plan.md D4). Expectation: the output of mruby 891d1d9d (Docker
# kishima/mruby:891d1d9d); rc2 answers several of these differently (.rc.out).
class DefInSdef
  def orig; :orig; end
  def self.go; def m; :m; end; end
  def self.ali; alias aliased orig; end
  def self.und; undef orig; end
end
DefInSdef.go
DefInSdef.ali
p [DefInSdef.new.m, DefInSdef.respond_to?(:m), DefInSdef.new.aliased]
DefInSdef.und
p DefInSdef.new.respond_to?(:orig)

k = Class.new { def self.go; def m; :class_new; end; end }
k.go
p [k.new.m, k.respond_to?(:m)]
c = Class.new { def pos; def q; :q; end; end }
c.new.pos
p [c.method_defined?(:q), Object.method_defined?(:q)]

K = :top
class ConstBase; K = :base; end
class ConstSub < ConstBase
  def self.plain; K; end
  class << self
    def in_sclass; K; end
  end
end
p [ConstSub.plain, ConstSub.in_sclass]

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
p [o.k, o.cv, Lex.under(Recv)]
Module.new.class_eval { [1].each { DefinedInBlock = 1 } }
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
p [v.private_method_defined?(:r), v.private_method_defined?(:d), v.respond_to?(:hidden), v.respond_to?(:hidden, true)]
other = Class.new
Class.new { private; other.attr_accessor :pub }
p other.public_method_defined?(:pub)

mod = Module.new { def mpub; end }
cls = Class.new { include mod; def pub; end; private def priv; end }
p [cls.method_defined?(:pub, false), cls.method_defined?(:mpub, false), cls.method_defined?(:mpub), cls.method_defined?(:priv, false)]

module ModfuncDef
  module_function
  def go; def m; :modfunc; end; end
end
ModfuncDef.go
p [Class.new { include ModfuncDef }.new.m, ModfuncDef.respond_to?(:m)]
