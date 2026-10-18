# The rc2-sweep causes of verdict A left after Y6 and fixed in docs/plans/rc2-sweep-fixes-plan.md
# Y6b (docs/verification/rc2-sweep/README.md, "What is left"), one section per cause, and
# `native-params` (the parameters of a native method as rc2 writes them). The Regexp one
# (`match-pos-past-end`) is in rc2_sweep_y6b_regexp.rb. Worklog
# docs/worklog/2026-10-18-rc2-sweep-fixes.md, Y6b.
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

# math-log-order: `math_log` reads both arguments ("f|f") before either domain
t("log(-1, nil)") { Math.log(-1, nil) }
t("log(-1, 'a')") { Math.log(-1, "a") }
t("log(-1, 2)") { Math.log(-1, 2) }
t("log(2, -1)") { Math.log(2, -1) }
t("log(8, 2)") { Math.log(8, 2) }

# alias-same-name: `mrb_alias_method` returns at once for two equal names; method_added fires
c = Class.new { def self.method_added(m); puts "added #{m}"; end }
t("alias_method(:x, :x)") { c.alias_method(:x, :x).equal?(c) }
t("x defined?") { c.method_defined?(:x) }
t("alias x x") { c.class_eval { alias x x }; :done }
t("alias_method(:y, :x)") { c.alias_method(:y, :x) }
t("frozen alias_method(:x, :x)") { Class.new.freeze.alias_method(:x, :x).class }

# undef-method-answer: `mrb_mod_undef` answers nil
t("undef_method") { Class.new.undef_method }
t("undef_method(:to_s)") { Class.new.undef_method(:to_s) }
t("undef_method(:nothing)") { Class.new.undef_method(:nothing) }

# shift-negative: the count, the frozen check, [] for an empty Array or 0, then the sign
t("shift(-1)") { a = [3, 1, 2]; [a.shift(-1), a] }
t("[].shift(-1)") { [].shift(-1) }
t("shift(0)") { [3, 1, 2].shift(0) }
t("shift(2)") { a = [3, 1, 2]; [a.shift(2), a] }
t("frozen shift(-1)") { [1].freeze.shift(-1) }
t("frozen [].shift(0)") { [].freeze.shift(0) }

# fiber-root-status: the root fiber is "created" until a switch comes back to it
def status(f) = (s = f.inspect; s[s.rindex("(")..])
t("root before") { status(Fiber.current) }
t("root alive?") { Fiber.current.alive? }
f = Fiber.new { Fiber.yield 1; 2 }
f.resume
t("root after a yield") { status(Fiber.current) }
t("fiber suspended") { status(f) }

# native-params: a C function's RProc is not strict, so required arguments are :opt too
t("1.method(:+)") { m = 1.method(:+); [m.arity, m.parameters] }
t("1.method(:to_s)") { m = 1.method(:to_s); [m.arity, m.parameters] }
t("[].method(:first)") { m = [].method(:first); [m.arity, m.parameters] }
t("[].method(:push)") { m = [].method(:push); [m.arity, m.parameters] }
t("[].method(:size)") { m = [].method(:size); [m.arity, m.parameters] }
t("''.method(:center)") { m = "".method(:center); [m.arity, m.parameters] }
t("Math.method(:log)") { m = Math.method(:log); [m.arity, m.parameters] }
t("Class#initialize") { m = Class.instance_method(:initialize); [m.arity, m.parameters] }
t("Module#initialize") { m = Module.instance_method(:initialize); [m.arity, m.parameters] }
t("Hash#keys") { m = {}.method(:keys); [m.arity, m.parameters] }
t("Integer#pow") { m = 1.method(:pow); [m.arity, m.parameters] }
k = Class.new { attr_accessor :v }
t("attr reader") { m = k.new.method(:v); [m.arity, m.parameters] }
t("attr writer") { m = k.new.method(:v=); [m.arity, m.parameters] }
