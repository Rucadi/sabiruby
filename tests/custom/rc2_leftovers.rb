# The five differences from mruby 4.1.0-rc2 that SabiRuby had before edge-diff-fix and kept
# through follow-master (docs/plans/rc2-leftovers-plan.md, L1-L5). Expectation: mruby
# 4.1.0-rc2 (.rc.out). L1: a constant is looked up from the running proc (`mrb_vm_const_get`,
# src/variable.c) — the proc's class and each enclosing one without ancestors, a singleton
# class's ancestors and the class it is attached to, then that class's ancestors — not from
# the class `class_eval` was given, and `const_missing` is asked of that class. L2/L4:
# `attr_*` is `mod_attr_define` (src/class.c): public under `private` and `module_function`,
# no `method_added`, nil as the answer, the instance variable name checked as
# `mrb_iv_name_sym_check` does. L3: `method_defined?` takes one argument and is
# `mrb_obj_respond_to` on the class (private methods too). L5: a class in a NameError is
# spelled as `Module#to_s` spells it, quoted where the reference quotes it; `Array#[]=` with a
# negative index says what `mrb_ary_set` / `mrb_ary_splice` / `mrb_range_beg_len` say.
# Addresses are taken out of messages (the two print different pointers), without a pattern so
# that the build without mruby-regexp runs this too.
def noaddr(s)
  out = ""
  i = 0
  while i < s.size
    if s[i, 2] == "0x"
      out << "0x"
      i += 2
      i += 1 while i < s.size && "0123456789abcdef".include?(s[i])
    else
      out << s[i]
      i += 1
    end
  end
  out
end
def msg
  p yield
rescue Exception => e
  p [e.class, noaddr(e.message)]
end

# L1
class A; X = :a; end
module M; X = :m; class B < A; def self.x; X; end; end; end
msg { M::B.x }
module Lex
  K2 = :lex
  def self.under(m); m.class_eval { K2 }; end
  def self.under2(m); m.class_eval { [1].map { K2 } }; end
  def self.under3(m); m.instance_eval { K2 }; end
  def self.under4(m); m.class_exec { K2 }; end
end
module Recv; K2 = :recv; end
module Recv2; end
msg { [Lex.under(Recv), Lex.under2(Recv), Lex.under3(Recv), Lex.under4(Recv), Lex.under(Recv2)] }
def topf(m); m.class_eval { RX }; end
module RR; RX = 1; end
msg { topf(RR) }
msg { RR.class_eval { RX } }
msg { RR.class_eval { [1].map { RX } } }
class C; Y = :c; class << self; def y; Y; end; end; end
class D < C; class << self; def y2; Y; end; end; end
msg { [C.y, D.y2] }
class E; class << self; Z = :sz; def z; Z; end; end; def self.z2; Z; end; end
msg { [E.z, E.z2] }
class F; def self.const_missing(n); [:cm, n]; end; def self.f; NOPE; end; end
msg { F.f }
module G; def self.const_missing(n); [:gcm, n]; end; class H; def self.h; NOPE2; end; end; end
msg { G::H.h }
module MO; def self.s; String; end; end
msg { MO.s }
c3 = Class.new(A)
c3.class_eval { def k2; X; end }
msg { c3.new.k2 }
msg { c3.class_eval { X } }
class A; def self.dm; define_method(:dmx) { X }; end; end
module NN; X = :nn; class AA < A; dm; end; end
msg { NN::AA.new.dmx }
module Mi; PI2 = 3; end
class Inc; include Mi; def self.g; PI2; end; end
msg { Inc.g }
module Outer; class Inner; include Mi; end; end
msg { Outer::Inner.class_eval { PI2 } }
msg { defined?(PI2) }
msg { Outer::Inner.class_eval { defined?(PI2) } }
class Bar; def self.const_missing(n); [:bcm, n]; end; end
msg { Bar::Nope }
msg { Object::Nope }
msg { Bar.singleton_class::Nope }
# a string given to `class_eval` reads from the class given; a `define_method` block keeps its own
# class for class variables too (`mrb_vm_cv_get` reads `MRB_PROC_TARGET_CLASS` as well)
module EvM; EX = 1; end
EX = 0
msg { [EvM.class_eval("EX"), EvM.module_eval("EX"), EvM.instance_eval("EX"), EvM.class_eval("[1].map { EX }")] }
class CvA; @@v = :a; def self.dm(k); k.define_method(:cv) { @@v }; end; def self.ds(o); o.define_singleton_method(:cv2) { @@v }; end; end
class CvB; @@v = :b; end
CvA.dm(CvB)
co = Object.new; CvA.ds(co)
msg { [CvB.new.cv, co.cv2] }
# a method of an included module reads from the module; a block made in it, from the include
# class, whose table is the module's and whose ancestors are the includer's (an origin shares
# its class's table the same way)
module Mx; MC = :mx; def m; MC; end; def blk; [1].map { [MC, SUPC] }; end; def bare; SUPC; end; end
class Sup; SUPC = :sup; end
class Inc2 < Sup; include Mx; end
msg { [Inc2.new.m, Inc2.new.blk] }
msg { Inc2.new.bare }
module Pp; def pm; [1].map { PPC }; end; end
class Pre; PPC = :pre; prepend Pp; def own; [1].map { PPC }; end; end
msg { [Pre.new.own, Pre.new.pm] }

# L2 and L4
msg { Class.new { p attr_accessor(:a); p attr_reader(:b, :c); p attr_writer(:d); p attr(:e) }.public_instance_methods(false).sort }
class PA
  private
  attr_accessor :q
  protected
  attr_reader :r
end
msg { [PA.public_instance_methods(false).sort, PA.private_instance_methods(false), PA.protected_instance_methods(false)] }
other = Class.new
Class.new { private; other.attr_accessor :pub }
msg { other.public_instance_methods(false).sort }
module MF
  module_function
  attr_reader :x
  def y; end
end
msg { [MF.respond_to?(:x), MF.respond_to?(:y), MF.private_instance_methods(false), MF.public_instance_methods(false)] }
class HA; def self.method_added(n); p [:added, n]; end; attr_accessor :z; def zz; end; end
msg { Class.new { attr_reader :"a?" } }
msg { Class.new { attr_reader :"@a" } }
msg { Class.new { attr_reader 1 } }
msg { Class.new { attr_reader :initialize }.private_instance_methods(false) }
o = Object.new
class << o; private; attr_reader :sx; end
msg { o.singleton_class.public_instance_methods(false) }

# L3
class MD
  def pub; end
  private def priv; end
  protected def prot; end
end
msg { [MD.method_defined?(:pub), MD.method_defined?(:priv), MD.method_defined?(:prot), MD.method_defined?("pub")] }
msg { [MD.method_defined?(:initialize), MD.method_defined?(:puts), MD.method_defined?(:nope)] }
msg { MD.method_defined?(:pub, true) }
msg { MD.method_defined?(:pub, false) }
msg { MD.method_defined? }
msg { MD.method_defined?(1) }

# L5
class DefInSdef
  def orig; :orig; end
  def self.ali; alias aliased orig; end
  def self.und; undef orig; end
end
msg { DefInSdef.ali }
msg { DefInSdef.und }
msg { DefInSdef.singleton_class.send(:remove_method, :zz) }
msg { DefInSdef.singleton_class.send(:undef_method, :zz) }
msg { DefInSdef.singleton_class.send(:alias_method, :a, :zz) }
msg { DefInSdef.singleton_class.send(:private, :zz) }
msg { DefInSdef.singleton_class.const_get(:Zz) }
msg { DefInSdef.singleton_class.class_eval { Zz } }
msg { Object.new.singleton_class.send(:alias_method, :a, :zz) }
msg { Class.new.send(:alias_method, :a, :zz) }
msg { DefInSdef.singleton_class.singleton_class.send(:alias_method, :a, :zz) }
msg { Module.new.singleton_class.send(:alias_method, :a, :zz) }
msg { Module.new { module_function :zz } }
msg { [DefInSdef.singleton_class.to_s, DefInSdef.singleton_class.singleton_class.inspect] }
a = [1, 2, 3]
msg { a[-5] = 1 }
msg { a[-4, 1] = 1 }
msg { a[-5, 0] = 1 }
msg { a[1, -1] = 1 }
msg { a[(-10..1)] = 1 }
msg { [].fill(1, -5, 1) }
msg { b = [1, 2, 3]; b[5..6] = 9; b }
msg { b = [1, 2, 3]; b[1..-1] = 9; b }
msg { b = [1, 2, 3]; b[-2..] = [7, 8, 9]; b }
msg { b = [1, 2, 3]; b[..1] = 0; b }
msg { b = [1, 2, 3]; b[3..1] = :x; b }
msg { b = [1, 2, 3]; b[-4..-5] = :x; b }
msg { b = [1, 2, 3]; b[5, 2] = 9; b }
msg { b = []; b[-1..] = 1 }
