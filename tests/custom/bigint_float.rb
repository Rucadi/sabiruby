# An Integer past 64 bits in `quo`, `remainder` and `Rational()` (rc2-sweep cause
# `bigint-float`). mruby 4.1.0-rc2 answers these by asking whether a value is a machine Integer
# and taking another road for a wide one:
#   n.quo(wide)     `int_quo` (src/numeric.c:329) answers a Rational when the divisor is an
#                   Integer (`mrb_integer_p`) and a Float otherwise, so `1.quo(2)` is (1/2)
#                   and `0.quo(2**70)` 0.0.
#   wide.quo(n)     `int_quo` reads the receiver with `mrb_integer(x)` (numeric.c:340)
#                   without looking at its type: `(2**70).quo(3)` is (0/1), `(2**70 + 5).quo(7)`
#                   (5/7).
#   n.remainder(wide)  `int_remainder` (mrbgems/mruby-numeric-ext/src/numeric_ext.c:55) takes
#                   the Float road for any divisor that is not a machine Integer (:82):
#                   `0.remainder(2**70)` is 0.0.
#   Rational(wide, f)  `rational_new_b` (mrbgems/mruby-rational/src/rational.c:253) narrows the
#                   denominator with `mrb_as_bint`, so 1.5 becomes 1: `Rational(2**70, 1.5)` is
#                   (1180591620717411303424/1), while `Rational(4, 1.5)` divides as Floats.
# The same question answers by the width of the number, and the receiver and the denominator
# are silently read as other values, so SabiRuby keeps the meaning the same at both widths:
# Rationals and Integers for exact operands, and `Rational(wide, f)` as `Rational(4, 1.5)` is
# answered (`a / b` as Floats, then the Float as a Rational), which is not CRuby's answer
# (2361183241434822606848/3) either (the author's decision of 2026-10-18, plan rc2-sweep-fixes).
# expected-from: SabiRuby (CRuby 3.2 says the same except for the two `Rational(_, 1.5)`, which
# it makes from 1.5 exactly: (2361183241434822606848/3) and (8/3)).
# The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end
t("0.quo(2**70)") { 0.quo(2**70) }
t("3.quo(2**70)") { 3.quo(2**70) }
t("(2**70).quo(3)") { (2**70).quo(3) }
t("(2**70 + 5).quo(7)") { (2**70 + 5).quo(7) }
t("(2**70).quo(2**70)") { (2**70).quo(2**70) }
t("0.remainder(2**70)") { 0.remainder(2**70) }
t("5.remainder(2**70)") { 5.remainder(2**70) }
t("Rational(2**70, 1.5)") { Rational(2**70, 1.5) }
# the machine-width cases are the same on both
t("1.quo(2)") { 1.quo(2) }
t("5.remainder(3)") { 5.remainder(3) }
t("Rational(4, 1.5)") { Rational(4, 1.5) }
