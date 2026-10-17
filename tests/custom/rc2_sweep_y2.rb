# The rc2-sweep causes of verdict A fixed in docs/plans/rc2-sweep-fixes-plan.md Y2
# (docs/verification/rc2-sweep/README.md, "A — follow rc2"), one section per cause, each
# call as the sweep made it or as the cause's rc2 source reads it (worklog
# docs/worklog/2026-10-18-rc2-sweep-fixes.md, Y2). The Regexp ones are in
# rc2_sweep_y2_regexp.rb, the CRuby answer of `fill` in fill_empty_range.rb.
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
t("range") { (1..3).__send__(:initialize, 0, "") }
t("range twice") { (1..3).__send__(:initialize, 0, 1) }
t("ary none") { a = [3, 1, 2]; a.__send__(:initialize); a }
t("ary 1 x") { a = [3, 1, 2]; a.__send__(:initialize, 1, :x); a }
t("ary block") { a = [3, 1, 2]; a.__send__(:initialize, 1) { |i| :y }; a }
t("ary -1") { a = [3, 1, 2]; a.__send__(:initialize, -1); a }
t("ary copy") { a = [3, 1, 2]; a.__send__(:initialize, [9], nil); a }
t("Array.new(-1)") { Array.new(-1) }
t("Array.new block") { Array.new(3) { |i| i * 2 } }
t("str") { s = +"abc"; s.__send__(:initialize); s }
t("class") { c = Class.new; c.__send__(:initialize).equal?(c) }
t("class arg") { Class.new.__send__(:initialize, 0) }
t("module") { m = Module.new; m.__send__(:initialize).equal?(m) }
t("module block") { m = Module.new; m.__send__(:initialize) { def mm; end }; m.instance_methods }

# argc-wide and argc-narrow: the counts rc2 declares and reads
t("instance_methods") { Class.new.instance_methods(nil, nil) }
t("constants") { Module.constants(nil, nil) }
t("sample nil nil") { [3, 1, 2].sample(nil, nil) }
t("sample 1 nil") { [3, 1, 2].sample(1, nil) }
t("data init") { Data.define(:x).new(x: 1).__send__(:initialize, 0, x: 1) }
t("hash frozen init") { {a: 1}.freeze.__send__(:initialize, 0) { } }

# frozen: where the frozen check stands
t("chomp! frozen") { "abc".freeze.chomp!("", nil) }
t("chomp! frozen nil") { "abc".freeze.chomp!(nil) }
t("delete frozen") { [3, 1, 2].freeze.delete(0) }
t("delete frozen block") { [3, 1, 2].freeze.delete(0) { :none } }
t("delete frozen found") { [3, 1, 2].freeze.delete(1) }
t("merge! frozen") { {a: 1}.freeze.merge!({}, nil) }
t("init_copy frozen") { {a: 1}.freeze.__send__(:initialize_copy, 0) }
t("aset frozen") { [3, 1, 2].freeze.__send__(:[]=, 0, -1, 9) }
t("aset frozen 2") { [3, 1, 2].freeze.__send__(:[]=, -9, 9) }
t("ary init frozen") { [3, 1, 2].freeze.__send__(:initialize, -1) }
t("str init frozen") { "abc".freeze.__send__(:initialize) }
t("hash init frozen") { {a: 1}.freeze.__send__(:initialize) }

# fiber-inspect
f = Fiber.new { Fiber.yield 1 }
t("fiber") { f }
f.resume
t("fiber suspended") { f.to_s }
f.resume
t("fiber terminated") { f }

# join-empty
t("join") { [[], 1].join(", ") }
t("join nested") { [1, [], [2, [], 3]].join("-") }
t("enum inspect") { 0.step([]) }
t("join recursive") { a = [1]; b = [a]; a << b; a.join(",") }

# method-missing-name
t("mm none") { Object.new.__send__(:method_missing) }
t("mm int") { Object.new.__send__(:method_missing, 0) }
t("mm str") { begin; Object.new.__send__(:method_missing, "foo", 1); rescue NoMethodError => e; [e.message, e.name, e.args]; end }

# binding-inspect
t("binding") { binding.inspect }

# conv-order
t("bigint pow") { (2**70) ** (2**70) }
t("pow mod nan") { (-3).pow(2**70, Float::NAN) }
t("caller") { caller(nil, 2**70) }
t("ldexp") { Math.ldexp(2**70, 2**70) }
t("bigint and nan") { (2**70) & Float::NAN }
t("bigint allbits inf") { (2**70).allbits?(Float::INFINITY) }

# visibility-args
t("public sym 0") { Class.new.__send__(:public, :sym, 0) }
t("private 0 sym") { Class.new.__send__(:private, 0, :sym) }
t("module_function") { Module.new.__send__(:module_function, :sym, 0) }

# merge-partial
t("merge partial") { h = {}; (h.merge!({a: 1}, nil) rescue nil); h }

# bigint-to-float
t("cos") { Math.cos(2**70) }
t("atan2") { Math.atan2(-1, 2**70) }
t("log base") { Math.log(1, 2**70) }
t("hypot") { Math.hypot(0, 2**70) }
t("sqrt rational") { Math.sqrt(Rational(1, 4)) }
t("remainder") { (-1.5).remainder(2**70) }
t("polar") { Complex.polar(1, 2**70) }

# lenient-arg
t("chomp nil") { "abc".chomp(nil) }
t("hash init_copy") { {}.__send__(:initialize_copy, 0) }
t("hash replace") { {}.replace(nil) }

# set-reinit
t("set reinit") { Set.new.__send__(:initialize) }
t("set frozen reinit") { Set[1].freeze.__send__(:initialize, [2]) }
t("set dup") { s = Set[1].dup; s << 2; s }

# time-year
t("gm -1") { Time.gm(-1).inspect }
t("gm 0") { Time.gm(0).to_s }
t("gm 5") { Time.gm(5).to_s }
t("asctime -1") { Time.gm(-1).asctime }

# proc-init-copy
t("proc init_copy") { proc { }.__send__(:initialize_copy, 0) }
t("proc init_copy proc") { pr = proc { 1 }; pr.__send__(:initialize_copy, proc { 2 }); pr.call }

# sum-block
t("sum block") { [1, 2, 3].sum { |x| x * 10 } }
t("sum owner") { [].method(:sum).owner }

# rational-minus
t("rational - str") { Rational(1, 2) - "" }
t("rational - nil") { Rational(1, 2) - nil }

# math-nan-domain
t("sqrt nan") { Math.sqrt(Float::NAN) }
t("acos nan") { Math.acos(Float::NAN) }
t("log nan") { Math.log(Float::NAN) }
t("log1p nan") { Math.log1p(Float::NAN) }

# negative-zero
t("modulo") { (-3).modulo(1.5) }
t("float %") { (-1.5) % 1.5 }

# split-empty
t("split") { "".split("abc", -1) }
t("split 1") { "".split("abc", 1) }
t("split awk -1") { " a\nb c ".split(nil, -1) }
t("split awk spaces") { "   ".split(" ", 2) }
t("split awk limit") { "a b  ".split(nil, 3) }

# int-div-nonfinite (the author's decision: both halves follow rc2)
t("div nan") { 0.div(Float::NAN) }
t("div inf") { 7.div(Float::INFINITY) }
t("div wide") { (-3).div(2**70) }
t("ceildiv wide") { (-3).ceildiv(2**70) }

# bytesplice: the forms rc2 reads
t("bs 0 -1") { s = +"abc"; s.bytesplice(0, -1) }
t("bs none") { "".bytesplice }
t("bs 1 arg") { "abc".bytesplice(0..1) }
t("bs 4 args") { "abc".bytesplice(0, 1, "x", 0) }
t("bs range str range") { s = +"abc"; s.bytesplice(0..1, "xyz", 1..2); s }
t("bs int int str") { s = +"abc"; s.bytesplice(1, 1, "XY"); s }
t("bs 5") { s = +"abc"; s.bytesplice(0, 2, "xyz", 2, 1); s }
t("bs neg len") { +"abc".bytesplice(0, -1, "x") }
t("bs out") { +"abc".bytesplice(4, 1, "x") }
t("bs range out") { +"abc".bytesplice(-5..1, "x") }
t("bs 3 range type") { +"abc".bytesplice(0..1, 1, 0..1) }
t("bs append") { s = +"abc"; s.bytesplice(3..3, "d"); s }
t("bs frozen") { "abc".freeze.bytesplice(0, 1, "x") }

# string-add-redefine: OP_ADD of two Strings does not look `+` up
class String
  alias __y2_plus +
  def +(o); "redefined"; end
end
t("str add") { "a" + "b" }
t("str send +") { "a".__send__(:+, "b") }
class String
  alias + __y2_plus
end

# Module#const_defined? with inherit false (found by Y1)
class Y2K; end
t("const_defined? false") { Y2K.const_defined?(:String, false) }
t("const_defined? true") { Y2K.const_defined?(:String) }
t("Object const_defined? false") { Object.const_defined?(:Comparable, false) }

# OP_SETCONST in a block run by instance_eval or class_eval: the class the block was written in
class Y2K
  "s".instance_eval { Y2A = 1 }
  String.class_eval { Y2B = 2 }
end
t("setconst") { [Y2K.constants(false).sort, String.const_defined?(:Y2B, false)] }
