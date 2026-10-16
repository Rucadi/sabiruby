# Which values can hold instance variables (docs/plans/rc2-ivar-plan.md, Q1). Expectation:
# mruby 4.1.0-rc2 (.rc.out). `obj_iv_p` (src/variable.c) is true only for OBJECT, CLASS,
# MODULE, SCLASS, HASH, CDATA and EXCEPTION. `mrb_iv_set` raises ArgumentError "cannot set
# instance variable" for anything else, before the frozen check (`mrb_obj_iv_set`), so a
# frozen String and an Integer say ArgumentError, not FrozenError; `mrb_iv_get` answers nil,
# `mrb_iv_defined` false, `instance_variables` [], and `mrb_iv_remove` undef, which
# `remove_instance_variable` turns into NameError. OP_SETIV / OP_GETIV and the attr methods go
# through the same functions. Struct and Data are MRB_TT_STRUCT, Set MRB_TT_SET, Random
# MRB_TT_ISTRUCT, Rational and Complex their own types: none holds one. Time, Method,
# Enumerator and Binding are CDATA or OBJECT and do.
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue => e
  puts "#{label} => #{e.class}: #{e.message}"
end

S = Struct.new(:a)
D = Data.define(:a)
class SubA < Array; end
class SubS < String; end
class SubH < Hash; end
class SubSet < Set; end
class SubR < Random; end
class E < StandardError; end
vals = [
  ["Object", Object.new], ["Class", Class.new], ["Module", Module.new],
  ["singleton class", Object.new.singleton_class], ["Hash", {}], ["Hash subclass", SubH.new],
  ["Exception", E.new], ["Time", Time.at(0)], ["Method", 1.method(:+)],
  ["UnboundMethod", Integer.instance_method(:+)], ["Enumerator", [1].each],
  ["String", "s"], ["String subclass", SubS.new("x")], ["frozen String", "f".freeze],
  ["Array", []], ["Array subclass", SubA.new], ["Range", 1..2], ["Proc", proc {}],
  ["lambda", lambda {}], ["big Integer", 2**70], ["Integer", 1], ["Float", 1.5],
  ["Symbol", :s], ["true", true], ["false", false], ["nil", nil],
  ["Struct", S.new(1)], ["Data", D.new(a: 1)], ["Fiber", Fiber.new {}],
  ["Rational", 1r], ["Complex", 1i], ["Random", Random.new(1)],
  ["Random subclass", SubR.new(1)], ["Set", Set.new], ["Set subclass", SubSet.new],
]
vals.each do |name, v|
  t("#{name} set") { v.instance_variable_set(:@x, 1) }
  t("#{name} get") { v.instance_variable_get(:@x) }
  t("#{name} defined?") { v.instance_variable_defined?(:@x) }
  t("#{name} ivars") { v.instance_variables }
  t("#{name} remove") { v.remove_instance_variable(:@x) }
end

# the instructions and the attr methods
class SubA
  attr_accessor :w
  def put; @y = 2; end
  def look; [@y, defined?(@y)]; end
end
a = SubA.new
t("SETIV on an Array subclass") { a.put }
t("GETIV on an Array subclass") { a.look }
t("attr_writer on an Array subclass") { a.w = 3 }
t("attr_reader on an Array subclass") { a.w }
class Integer; def put_iv; @z = 1; end; def get_iv; @z; end; end
t("SETIV on an Integer") { 5.put_iv }
t("GETIV on an Integer") { 5.get_iv }
class SubS; def initialize(s); super; @n = 1; end; end
t("@x in initialize of a String subclass") { SubS.new("q") }

# the order of the checks
o = Object.new.freeze
t("frozen Object set") { o.instance_variable_set(:@x, 1) }
t("frozen Object remove") { o.remove_instance_variable(:@x) }
t("bad name on a String") { "s".instance_variable_set(:x, 1) }
t("frozen Array remove") { [].freeze.remove_instance_variable(:@x) }

# copies: a Hash copies its ivars, an Exception and an Object too
h = {}; h.instance_variable_set(:@k, 1)
t("Hash dup") { h.dup.instance_variables }
t("Hash clone") { h.clone.instance_variable_get(:@k) }
e = E.new("m"); e.instance_variable_set(:@k, 2)
t("Exception dup") { e.dup.instance_variable_get(:@k) }
t("Set dup") { Set.new([1]).dup.instance_variables }
t("Random dup") { r = Random.new(3); r.dup.rand(100) == r.rand(100) }
