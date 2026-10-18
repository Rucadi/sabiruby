# The rc2-sweep causes of verdict B fixed in docs/plans/rc2-sweep-fixes-plan.md Y4
# (docs/verification/rc2-sweep/README.md, "B — wording only"), one section per cause, each
# call as the sweep made it or as the cause's rc2 source reads it (worklog
# docs/worklog/2026-10-18-rc2-sweep-fixes.md, Y4), and what Y4 took from outside the tables
# (Class.allocate, an uninitialized Range, Exception#exception).
# expected-from: the reference (.expected is .rc.out).
# addresses written as 0x (no Regexp: the case runs in the build without mruby-regexp too)
def mask(s)
  out = +""
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

def t(label)
  r = yield
  puts "#{label}: #{mask(r.inspect)}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{mask(e.message)}"
end

# reinitialize: initialize called again on a value

puts "-- conv-float-msg, conv-int-msg, conv-msg-other: mrb_ensure_float_type, mrb_int_noconv, mrb_ensure_int_type"
t("1 + true") { 1 + true }
t("1 - nil") { 1 - nil }
t("1 * nil") { 1 * nil }
t("1 / nil") { 1 / nil }
t("(2**70) / nil") { (2**70) / nil }
t("(2**70) + nil") { (2**70) + nil }
t("1 % 'a'") { 1 % "a" }
t("1.divmod('a')") { 1.divmod("a") }
t("1.div(nil)") { 1.div(nil) }
t("1.div(Complex(2, 0))") { 1.div(Complex(2, 0)) }
t("1.fdiv(:a)") { 1.fdiv(:a) }
t("1.quo('a')") { 1.quo("a") }
t("1 & nil") { 1 & nil }
t("1 | 1.5") { 1 | 1.5 }
t("(2**70) ^ nil") { (2**70) ^ nil }
t("1 ** nil") { 1 ** nil }
t("(2**70) ** nil") { (2**70) ** nil }
t("(2**70) ** Rational(1, 2)") { (2**70) ** Rational(1, 2) }
t("1.5 + true") { 1.5 + true }
t("1.5 * nil") { 1.5 * nil }
t("1.5 % 'a'") { 1.5 % "a" }
t("1.5 ** nil") { 1.5 ** nil }
t("1.5.divmod(nil)") { 1.5.divmod(nil) }
t("1.5.quo([])") { 1.5.quo([]) }
t("1.5 % Complex(2, 0)") { 1.5 % Complex(2, 0) }
t("Math.sqrt(true)") { Math.sqrt(true) }
t("Math.atan2(nil, 1)") { Math.atan2(nil, 1) }
t("Math.sqrt(Complex(4, 0))") { Math.sqrt(Complex(4, 0)) }
t("Math.sqrt(Complex(4, 1))") { Math.sqrt(Complex(4, 1)) }
t("format('%f', 'a')") { format("%f", "a") }
t("format('%f', nil)") { format("%f", nil) }
t("[1, 'a'].sum") { [1, "a"].sum }
t("Rational(1, 2) / nil") { Rational(1, 2) / nil }
t("Rational(1, 2).quo(true)") { Rational(1, 2).quo(true) }
t("Rational(1, 2) ** nil") { Rational(1, 2) ** nil }
t("Rational(nil)") { Rational(nil) }
t("Rational(2**70, 'a')") { Rational(2**70, "a") }
t("Complex(nil)") { Complex(nil) }
t("Complex(1, 2) + nil") { Complex(1, 2) + nil }
t("Complex(1, 2) / :a") { Complex(1, 2) / :a }
t("Complex(1, 2) ** nil") { Complex(1, 2) ** nil }
t("3.pow(2**70, nil)") { 3.pow(2**70, nil) }

puts "-- msg-other: Rational"
t("Rational(1, 2) / 0") { Rational(1, 2) / 0 }
t("Rational(1, 2) / Rational(0, 1)") { Rational(1, 2) / Rational(0, 1) }
t("Rational(1, 2) ** (2**70)") { Rational(1, 2) ** (2**70) }
t("Rational(2**70, 3) ** (2**70)") { Rational(2**70, 3) ** (2**70) }

puts "-- module-required, class-new-super: mrb_check_type(MODULE), ensure_class_type, mrb_check_inheritable"
t("Module.new.include(nil)") { Module.new.include(nil) }
t("Module.new.include(String)") { Module.new.include(String) }
t("Class.new.prepend(k: 1)") { Class.new.prepend(k: 1) }
t("Object.new.extend(1)") { Object.new.extend(1) }
t("Module.new.include?(nil)") { Module.new.include?(nil) }
t("Module.new.include?(String)") { Module.new.include?(String) }
t("1.is_a?(1)") { 1.is_a?(1) }
t("1.kind_of?(nil)") { 1.kind_of?(nil) }
t("1.instance_of?(:a)") { 1.instance_of?(:a) }
t("Class.new(0)") { Class.new(0) }
t("Class.new(nil)") { Class.new(nil) }
t("Class.new(Kernel)") { Class.new(Kernel) }
t("Class.new(Class)") { Class.new(Class) }
t("class < Class") { eval("class Y4Sub < Class; end") }

puts "-- type-nan-name: mrb_check_type of a Float (word boxing)"
[1.5, Float::NAN, Float::INFINITY, -0.0, 1e100, 1e-100, 2.0**256, 2.0**-255].each do |f|
  t("{}.default_proc = #{f}") { {}.default_proc = f }
end
t("[].product(Float::NAN)") { [].product(Float::NAN) }
t("[].product(1.5)") { [].product(1.5) }
t("Module.new.send(:private, Float::NAN)") { Module.new.send(:private, Float::NAN) }
t("Class.new.define_method(:a, nil)") { Class.new.define_method(:a, nil) }
t("Class.new.define_method(:a, 1)") { Class.new.define_method(:a, 1) }

puts "-- not-a-symbol, msg-other: Binding"
t("binding.local_variable_get(nil)") { binding.local_variable_get(nil) }
t("binding.local_variable_set(1, 1)") { binding.local_variable_set(1, 1) }
t("binding.local_variable_get('')") { binding.local_variable_get("") }
t("binding.local_variable_defined?('Abc')") { binding.local_variable_defined?("Abc") }
t("binding.local_variable_get('nope')") { binding.local_variable_get("nope") }

puts "-- unknown-keyword, data-init, struct-const-name"
Y4D = Data.define(:x, :y)
t("[].sample(k: 1)") { [].sample(k: 1) }
t("[1].shuffle('s' => 1)") { [1].shuffle("s" => 1) }
t("Y4D.new(x: 1, y: 2).with(k: 1)") { Y4D.new(x: 1, y: 2).with(k: 1) }
t("Y4D.new(x: 1, y: 2).initialize") { Y4D.new(x: 1, y: 2).send(:initialize) }
t("Y4D.new(y: 2)") { Y4D.new(y: 2) }
t("Y4D.new(x: 1, k: 2)") { Y4D.new(x: 1, k: 2) }
t("Y4D.new(x: 1, y: 2, k: 3)") { Y4D.new(x: 1, y: 2, k: 3) }
t("def f(a:) end; f(a: 1, b: 2)") { def y4f(a:) end; y4f(a: 1, b: 2) }
t("Struct.new('abc')") { Struct.new("abc") }

puts "-- Class.allocate"
c = Class.allocate
t("Class.allocate.new") { c.new }
t("Class.allocate.allocate") { c.allocate }
t("Class.allocate.superclass") { c.superclass }
t("Class.allocate.ancestors.size") { c.ancestors.size }
t("Class.allocate def") { c.class_eval { def y4m; end }; c.instance_methods(false) }
t("Class.new(Class.allocate)") { Class.new(c).class }

puts "-- an uninitialized Range"
r = Range.allocate
%i[begin end first last exclude_end? inspect to_s size to_a each].each do |m|
  t("Range.allocate.#{m}") { r.__send__(m) }
end
%i[== eql? === include? cover?].each do |m|
  t("Range.allocate.#{m}(1..2)") { r.__send__(m, 1..2) }
end
t("(1..2) == Range.allocate") { (1..2) == r }
t("(1..2).eql?(Range.allocate)") { (1..2).eql?(r) }
t("(1..2).cover?(Range.allocate)") { (1..2).cover?(r) }
t("Range.allocate == itself") { r == r }
t("Range.allocate.dup") { r.dup }
t("[1, 2][Range.allocate]") { [1, 2][r] }
t("[1, 2][Range.allocate] = 1") { [1, 2][r] = 1 }
t("'ab'[Range.allocate]") { "ab"[r] }
t("'ab'.byteslice(Range.allocate)") { "ab".byteslice(r) }
t("[1, 2].fill(0, Range.allocate)") { [1, 2].fill(0, r) }
t("rand(Range.allocate)") { rand(r) }

puts "-- Exception#exception: mrb_obj_clone, mrb_exc_mesg_set"
e = RuntimeError.new("a")
def e.hi; :hi; end
e.instance_variable_set(:@x, 1)
t("e.exception('b').hi") { e.exception("b").hi }
t("e.exception('b') message, @x") { x = e.exception("b"); [x.message, x.instance_variable_get(:@x), x.equal?(e)] }
t("e.exception(nil).message") { e.exception(nil).message }
t("e.exception(1).message") { e.exception(1).message }
t("e.exception.equal?(e)") { e.exception.equal?(e) }
f = RuntimeError.new("f").freeze
t("frozen.exception('g')") { x = f.exception("g"); [x.message, x.frozen?] }

puts "-- eval-error-line: the line of the first diagnostic (Host::compile_eval)"
t("eval line 3") { eval("x = 1\ny = 2\n1 +") }
t("eval from line 10") { eval("1 +", nil, "f.rb", 10) }
t("eval generator error") { eval("x = 1\nalias $a $b", nil, "f.rb", 10) }
t("instance_eval line 2") { Object.new.instance_eval("x = 1\n1 +") }

puts "-- mruby's limits: Integer#fdiv by zero, Rational() of a wide Integer and a NaN or an infinity"
[0, 0.0, -0.0, Rational(0, 1), Complex(0, 0)].each do |d|
  t("7.fdiv(#{d.inspect})") { 7.fdiv(d) }
  t("(2**70).fdiv(#{d.inspect})") { (2**70).fdiv(d) }
end
t("1.5.fdiv(0)") { 1.5.fdiv(0) }
[Float::NAN, Float::INFINITY, -Float::INFINITY].each do |f|
  t("Rational(2**70, #{f})") { Rational(2**70, f) }
  t("Rational(#{f}, 2**70)") { Rational(f, 2**70) }
end
