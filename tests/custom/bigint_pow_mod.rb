# `Integer#pow(n, m)` answers the same for a receiver or an exponent past 64 bits as for small
# ones (rc2-sweep cause `bigint-pow-mod`). mruby 4.1.0-rc2's `int_powm`
# (mrbgems/mruby-numeric-ext/src/numeric_ext.c:199) refuses a Float modulus on the small road
# ("int.pow(n,m): m must be integer") and answers 0 for a modulus of 1, but sends a wide receiver,
# exponent or modulus to `mrb_bint_powm` (mrbgems/mruby-bigint/core/bigint.c:5982), which narrows a
# Float modulus with `mrb_as_bint` (1.5 becomes 1, so the answer is 0) and does not reduce x**0 by
# a modulus of 1 (`(2**70).pow(0, 1)` is 1). Silently wrong values that depend on the operands'
# size (the compatibility policy, plan rc2-sweep-fixes Y6b, 2026-10-18).
# expected-from: SabiRuby (CRuby 3.2 refuses the Float modulus too, in its own words, with
# RangeError for the negative exponent it looks at first, and answers 0 for x**0 mod 1).
# The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

t("(2**70).pow(2, 1.5)") { (2**70).pow(2, 1.5) }
t("7.pow(2**70, 1.5)") { 7.pow(2**70, 1.5) }
t("(-3).pow(2**70, 1.5)") { (-3).pow(2**70, 1.5) }
t("(2**70).pow(-1, 1.5)") { (2**70).pow(-1, 1.5) }
t("7.pow(2, 1.5)") { 7.pow(2, 1.5) }
t("(2**70).pow(0, 1)") { (2**70).pow(0, 1) }
t("7.pow(0, 1)") { 7.pow(0, 1) }
t("(2**70).pow(2, 7)") { (2**70).pow(2, 7) }
