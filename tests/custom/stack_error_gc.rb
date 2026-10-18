# The message of SystemStackError after a GC (rc2-sweep cause `stack-error-gc`). In mruby
# 4.1.0-rc2 every SystemStackError is one object made in advance (`mrb->stack_err`), and the
# collector's final marking clears its message and backtrace (`clear_error_object`,
# src/gc.c:1484, called at :1528) once a GC has run, so the message is "stack level too deep"
# before the first GC and "SystemStackError" (the class name) after it. An answer that depends
# on the collector's state is a defect under the compatibility policy, so SabiRuby always says
# "stack level too deep" (found in plan rc2-sweep-fixes Y4, C by the policy, 2026-10-18).
# expected-from: SabiRuby (CRuby 3.2 says the same). The reference's answers are in .rc.out.
# README, "Known deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end
def f; f; end
t("before a GC") { f }
GC.start
t("after GC.start") { f }
