# `Integer#div` by a Float that is not a whole number (rc2-sweep cause `int-div-float`). In
# mruby 4.1.0-rc2 `int_idiv` (src/numeric.c:299) turns the divisor into a machine integer with
# `mrb_as_int` before dividing, so 1.5 becomes 1: `7.div(1.5)` is 7 and `(-3).div(1.5)` is -3.
# `ceildiv` (mrbgems/mruby-numeric-ext/mrblib/numeric_ext.rb:109, `-div(-other)`) inherits it.
# The same limit refuses a divisor it cannot narrow (NaN, Infinity, 2**70 raise RangeError, and
# SabiRuby refuses them too, `rc2_sweep_y2.rb`), but here it answers a silently wrong value,
# which the compatibility policy ("mruby's limits are the specification") counts as a defect.
# SabiRuby divides by the Float and floors, as `(7 / 1.5).floor` does on both (plan
# rc2-sweep-fixes Y5, 2026-10-18). The wide receiver's slip of the same kind is
# `bigint_reference_bugs.rb`.
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
t("7.div(1.5)") { 7.div(1.5) }
t("(-3).div(1.5)") { (-3).div(1.5) }
t("7.ceildiv(1.5)") { 7.ceildiv(1.5) }
t("(7 / 1.5).floor") { (7 / 1.5).floor }
# a whole Float is the same on both
t("7.div(2.0)") { 7.div(2.0) }
