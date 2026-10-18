# `Float#remainder` of a large receiver loses the remainder, in mruby 4.1.0-rc2 and in SabiRuby
# alike (rc2-sweep row `float-remainder-precision`). Both compute `a - b * trunc(a / b)` in
# Floats: rc2's `flo_remainder` (mrbgems/mruby-numeric-ext/src/numeric_ext.c:424, the line :432),
# which `Integer#remainder` also takes for a Float divisor (`int_remainder`, :55, :68), and
# SabiRuby's `flo_remainder` (src/builtins/ext_numeric.rs). For 2**70 = 1.5 * q + 1 the product
# rounds and the answer is 0.0. This is not a difference from the reference, so it is kept (the
# author's decision of 2026-10-19, plan release-0.8 R1): the test holds today's answer, which is
# the reference's, so that a change to it is seen. README, "Known deviations" (a defect kept as
# rc2 has it).
# expected-from: the reference (4.1.0-rc2). CRuby 3.2 answers 1.0, 1.0 and -1.0 for the first
# three lines (it uses fmod) and the same as here for the rest; `%` is 1.0 on all three VMs.
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

t("(2**70).remainder(1.5)") { (2**70).remainder(1.5) }
t("(2.0**70).remainder(1.5)") { (2.0**70).remainder(1.5) }
t("(-(2.0**70)).remainder(1.5)") { (-(2.0**70)).remainder(1.5) }
t("(2**70) % 1.5") { (2**70) % 1.5 }
t("(2.0**70) % 1.5") { (2.0**70) % 1.5 }
t("7.0.remainder(1.5)") { 7.0.remainder(1.5) }
t("(-7.0).remainder(1.5)") { (-7.0).remainder(1.5) }
