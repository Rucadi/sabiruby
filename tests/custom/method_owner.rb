# Where each method is defined (rc2-sweep cause `method-owner`, plan rc2-sweep-fixes Y3): the
# methods SabiRuby defined in another class or module than 4.1.0-rc2 does, moved to rc2's owner.
# What moving changes: `method(:x).owner`, `instance_methods(false)`, where `super` goes, the
# order of the lookup, and the behaviour of the method rc2 has there (Comparable's `==` and
# `between?` are mrblib compar.rb, Enumerable's `take`/`drop`/`hash`/`to_a` are mrblib and
# mruby-enum-ext, Kernel's `dup`/`freeze`/`===` are kernel.c, `Module.new` and `Range.new` are
# `Class#new`). The book prints `1.method(:between?).owner` as Comparable (corelib.re:137, 349).
# expected-from: the reference (.expected is .rc.out).
def t
  p yield
rescue Exception => e
  p [e.class, e.message]
end

# owners, as `method(:x).owner` answers them
t { [1.method(:between?).owner, 1.0.method(:between?).owner, Rational(1, 2).method(:between?).owner] }
t { [1.method(:<).owner, 1.method(:<=>).owner, 1.0.method(:>=).owner, Rational(1, 2).method(:<).owner, Complex(1, 0).method(:<=).owner] }
t { [1.0.method(:finite?).owner, 1.0.method(:infinite?).owner, 1.method(:finite?).owner] }
t { [1.method(:-@).owner, 1.0.method(:-@).owner, 1.method(:eql?).owner, 1.0.method(:eql?).owner, Rational(1, 2).method(:eql?).owner] }
t { [Numeric.instance_method(:==).owner, Numeric.instance_method(:<=>).owner, Numeric.instance_method(:eql?).owner] }
t { [[].method(:take).owner, [].method(:drop).owner, [].method(:hash).owner, {}.method(:to_a).owner, {}.method(:hash).owner] }
t { [[].method(:dup).owner, [].method(:freeze).owner, [].method(:frozen?).owner, {}.method(:dup).owner, "".method(:dup).owner, (1..2).method(:dup).owner] }
t { ["".method(:===).owner, :a.method(:===).owner, :a.method(:hash).owner, "".method(:freeze).owner, "".method(:frozen?).owner] }
t { pr = proc { }; [pr.method(:dup).owner, pr.method(:clone).owner, Proc.instance_method(:initialize).owner] }
t { [Exception.new.method(:==).owner, Module.new.method(:==).owner, Module.new.method(:dup).owner, Module.instance_method(:initialize_copy).owner] }
t { [nil.method(:nil?).owner, 1.method(:nil?).owner, Object.new.method(:__id__).owner] }
t { [Range.method(:new).owner, Module.method(:new).owner, Class.method(:new).owner] }

# what each class defines itself
t { [:<, :<=, :>, :>=, :<=>, :==, :between?, :eql?, :-@, :finite?, :infinite?].map { |m| [Numeric, Integer, Float].map { |c| c.instance_methods(false).include?(m) } } }
t { [:take, :drop, :hash, :dup, :freeze, :frozen?].map { |m| Array.instance_methods(false).include?(m) } }
t { [:to_a, :hash, :dup, :freeze, :frozen?].map { |m| Hash.instance_methods(false).include?(m) } }
t { [String.instance_methods(false).include?(:===), Symbol.instance_methods(false).include?(:hash), Exception.instance_methods(false).include?(:==)] }
t { [Module.instance_methods(false).include?(:==), Module.instance_methods(false).include?(:dup), Module.private_instance_methods(false).include?(:initialize_copy)] }
t { [Kernel.instance_methods(false).include?(:__id__), NilClass.instance_methods(false).include?(:nil?), Range.singleton_methods(false), Module.singleton_methods(false).include?(:new)] }

# where `super` goes
class Integer; def between?(a, b); [:int, super]; end; end
t { 1.between?(0, 2) }
class Integer; remove_method :between?; end
module Comparable; alias __y3_between between?; def between?(a, b); [:cmp, __y3_between(a, b)]; end; end
t { [1.between?(0, 2), 1.0.between?(2, 3)] }
module Comparable; alias between? __y3_between; remove_method :__y3_between; end
class A2 < Array; def initialize_copy(o); $ic = o.size; super; end; end
class A3 < Array; def initialize_copy(o); end; end
class S3 < String; def initialize_copy(o); end; end
t { [A2.new([1, 2]).dup, $ic, A3.new([1]).dup, S3.new("ab").dup, A2.new([3]).clone.class] }
class Cm; def <=>(o); [:sup, super]; end; end
t { c = Cm.new; [c <=> c, c <=> Cm.new] }

# behaviour that comes with rc2's owner
t { 1.between?("a", 2) }
t { 1.0.between?(Float::NAN, 2) }
t { Rational(1, 2) < "a" }
t { [Rational(1, 2) < 1, Rational(1, 2).between?(0, 1), Complex(1, 0) <= 2] }
t { Complex(1, 1) < 2 }
t { Numeric.new <=> Numeric.new }
t { n = Numeric.new; [n <=> n, n == n, Numeric.new == Numeric.new] }
t { Numeric.new < 1 }
class Cn; include Comparable; end
t { x = Cn.new; [Cn.new == Cn.new, x == x, Cn.new <=> Cn.new] }
t { [1.eql?(1), 1.eql?(1.0), 1.0.eql?(1.0), (2**70).eql?(2**70), (2**70).eql?(2.0**70), Rational(1, 1).eql?(1), Rational(1, 2).eql?(Rational(1, 2)), Numeric.new.eql?(Numeric.new)] }
t { [-(2**63), -(2**70), -(1.5), -(0.0)] }
t { x = 3; y = 2.5; [-x, -y] }
t { [ArgumentError.new("a") == ArgumentError.new("a"), RuntimeError.new == RuntimeError.new] }
t { e = Exception.new; re = RuntimeError.new; [e.exception.equal?(e), e.exception(e).equal?(e), re.exception("m").equal?(re), re.exception("m").message] }
t { [[1, 2].take(1), [1, 2].drop(1), [1, 2].take(1.9), {a: 1}.to_a] }
t { [1].take(-1) }
t { [1].drop(-1) }
t { a = [1]; a << a; a.hash == a.hash }
t { [[1, 2].hash == [1, 2].hash, {[1, 2] => 3}[[1, 2]]] }
t { pr = proc { 1 }; [pr.dup.call, pr.clone.call, pr.dup.class] }
t { a = [1].freeze; [a.dup.frozen?, a.clone.frozen?, "x".freeze.dup.frozen?] }
t { [:a === :a, "a" === "a", "a" === :a] }
t { Proc.new { 1 }.__send__(:initialize) }
t { Module.new.__send__(:initialize_copy, String) }
t { c = Class.new { def self.k; 1; end }; c.freeze; d = c.dup; [d.k, d.frozen?, c.clone.frozen?] }
class MyMod < Module; end
t { m = MyMod.new { def self.q; 1; end }; [m.class, m.q] }
t { Module.new(1) }
t { m = Module.new { def z; end }; m.instance_methods }
class R2 < Range; def initialize(a, b); super(a, b, true); end; end
t { R2.new(1, 3).to_a }
t { [Range.new(1, 2, true), Range.new(1, 2).class] }
t { Range.new(1) }
t { Range.new(1, "a") }
