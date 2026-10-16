# `__method__`, `super` and `defined?(super)` inside a string handed to `eval`. In mruby
# 4.1.0-rc2 the string runs on a frame of its own that `mrb_exec_irep()` gives the `mid` of the
# C frame it is pushed over, which is `eval` (mrbgems/mruby-eval/src/eval.c): `__method__` at
# the top level answers `:eval`, `super` looks for the superclass method of `Kernel#eval`
# (ArgumentError or nil), and `defined?(super)` answers "super" in a method with no superclass
# method. mruby master b0b1de54e fixed it as a bug ("name the string's frame for the method
# that called `eval`"); SabiRuby already answered that way.
# expected-from: mruby master b0b1de54e (image kishima/mruby:891d1d9d); CRuby 3.2 agrees. The
# reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue Exception => e
  puts "#{label} !! #{e.class}: #{e.message}"
end
t("top eval __method__") { eval("__method__") }
def mm; eval("__method__"); end
t("method eval __method__") { mm }
class Base; def m(x); [:base, x]; end; end
class Sub < Base; def m(x); eval("super(x + 10)"); end; end
t("eval super") { Sub.new.m(1) }
class Lone; def m; eval("defined?(super)"); end; end
t("eval defined?(super)") { Lone.new.m }
class Sub2 < Base; def m(x); eval("defined?(super)"); end; end
t("eval defined?(super) with one") { Sub2.new.m(1) }
