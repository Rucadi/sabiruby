# The receivers and argument values of the method sweep (docs/plans/rc2-sweep-plan.md X1).
# Each entry is a lambda so that every call gets a fresh one: a destructive method does not
# carry its effect to the next call, and an argument the method changes is not reused.
SW_S = Struct.new(:a, :b)
SW_D = Data.define(:x, :y)

# The argument values the plan lists, in its order (22). Their index is what a case names.
SW_VALS = [
  -> { nil }, -> { true }, -> { 0 }, -> { -1 }, -> { 1 }, -> { 2**70 }, -> { 1.5 },
  -> { Float::NAN }, -> { Float::INFINITY }, -> { "" }, -> { "abc" }, -> { "あ" }, -> { :sym },
  -> { [] }, -> { [1, 2] }, -> { {} }, -> { {a: 1} }, -> { 1..3 }, -> { proc { |*x| x } },
  -> { SW_S.new(1, 2) }, -> { "frz".freeze }, -> { [1, 2].freeze },
]

# Receivers by the class whose methods they reach: empty and with contents, frozen where a
# value can be. A class's own singleton methods are called on the class itself (SW_SELF).
SW_RECV = {
  "nil" => -> { nil }, "true" => -> { true }, "false" => -> { false },
  "Integer:0" => -> { 0 }, "Integer:7" => -> { 7 }, "Integer:-3" => -> { -3 },
  "Integer:big" => -> { 2**70 },
  "Float:0.0" => -> { 0.0 }, "Float:2.5" => -> { 2.5 }, "Float:-1.5" => -> { -1.5 },
  "Float:inf" => -> { Float::INFINITY },
  "Rational" => -> { Rational(1, 2) }, "Rational:-3" => -> { Rational(-3, 1) },
  "Complex" => -> { Complex(1, 2) }, "Complex:real" => -> { Complex(1.5, 0) },
  "String:empty" => -> { +"" }, "String:abc" => -> { +"abc" }, "String:frozen" => -> { "abc".freeze },
  "String:mb" => -> { +"あいう" }, "String:ws" => -> { +" a\nb c " },
  "Symbol" => -> { :abc }, "Symbol:op" => -> { :+ },
  "Array:empty" => -> { [] }, "Array:int" => -> { [3, 1, 2] }, "Array:mixed" => -> { [1, [2, nil], "a"] },
  "Array:frozen" => -> { [3, 1, 2].freeze },
  "Hash:empty" => -> { {} }, "Hash:mixed" => -> { {a: 1, "b" => 2} }, "Hash:frozen" => -> { {a: 1}.freeze },
  "Hash:default" => -> { Hash.new { |h, k| k } },
  "Range:int" => -> { 1..3 }, "Range:excl" => -> { 1...10 }, "Range:str" => -> { "a".."c" },
  "Range:endless" => -> { (1..) }, "Range:float" => -> { 1.5..3 },
  "Set:empty" => -> { Set.new }, "Set:int" => -> { Set[3, 1, 2] }, "Set:frozen" => -> { Set[1].freeze },
  "Proc" => -> { proc { |a, b| [a, b] } }, "Proc:lambda" => -> { lambda { |a| a } },
  "Method" => -> { 1.method(:+) }, "Method:push" => -> { [].method(:push) },
  "UnboundMethod" => -> { Array.instance_method(:push) },
  "Struct" => -> { SW_S.new(1, 2) }, "Struct:frozen" => -> { SW_S.new(1, 2).freeze },
  "Data" => -> { SW_D.new(x: 1, y: 2) },
  "Exception" => -> { StandardError.new("msg") }, "Exception:plain" => -> { RuntimeError.new },
  "NameError" => -> { NameError.new("msg", :foo) },
  "NoMethodError" => -> { NoMethodError.new("msg", :foo, [1]) },
  "StopIteration" => -> { StopIteration.new("msg") },
  "UncaughtThrowError" => -> { begin; throw :x, 1; rescue UncaughtThrowError => e; e; end },
  "Enumerator" => -> { [1, 2, 3].each }, "Enumerator:slice" => -> { (1..3).each_slice(2) },
  "Enumerator::Lazy" => -> { (1..3).lazy }, "Enumerator::Chain" => -> { [1].chain([2]) },
  "MatchData" => -> { /(a)(b)?/.match("xab") },
  "Regexp" => -> { /a(b)?/i }, "Regexp:plain" => -> { /x/ },
  "Time" => -> { Time.at(0).utc }, "Time:usec" => -> { Time.at(1700000000, 123456).utc },
  "Random" => -> { Random.new(1) },
  "Fiber" => -> { Fiber.new { |x| Fiber.yield x; 2 } },
  "Binding" => -> { x = 1; binding },
  "Object" => -> { Object.new },
  "Module" => -> { Module.new }, "Class" => -> { Class.new },
}
SW_SELF = %w[Kernel Math GC ObjectSpace Integer Float String Symbol Array Hash Range Set Proc
  Struct Data Exception Time Random Fiber Regexp Module Class Object Comparable Rational Complex
  Method UnboundMethod Enumerator Binding MatchData NilClass TrueClass FalseClass]
SW_SELF.each { |n| SW_RECV["self:#{n}"] = Object.const_get(n).then { |c| -> { c } } }

# How many times the sweep's block may be called before it stops the call (raising SwStop, an
# Exception outside StandardError so that a method's own `rescue` does not take it). No finite
# call of the table yields more: the receiver or argument with the most elements is 1...10
# (9 values), and a call takes at most 3 arguments (cases.py MAXARGS), 9**3 at most for
# `product` and the like. A call stopped there iterates towards 2**70 or Infinity, or forever:
# without the stop it grows the block's record until memory runs out, and what each VM does
# when memory runs out is not what the sweep compares.
SW_YIELD_MAX = 9**3
class SwStop < Exception; end

def sw_esc(s)
  s.gsub("\n", "\\n").gsub("\t", "\\t").gsub("\r", "\\r")
end

def sw_insp(v)
  s = v.inspect
  s.is_a?(String) ? sw_esc(s) : "<inspect not a String: #{s.class}>"
rescue Exception => e
  "<inspect raised #{e.class}>"
end
