# An Integer past 64 bits with a Float in `&`, `|`, `^`, `allbits?`, `anybits?` and `nobits?`
# raises TypeError, as a small Integer does (rc2-sweep cause `bigint-bitop-float`). In mruby
# 4.1.0-rc2 a small receiver refuses the Float ("can't convert Float into Integer"), but a wide one
# takes mruby-bigint's road (`mrb_bint_and`, `mrb_bint_or`, `mrb_bint_xor`,
# mrbgems/mruby-bigint/core/bigint.c:6076-6124), which narrows the other operand with
# `mrb_as_bint` (:5517): 1.5 becomes 1, so `(2**70) & 1.5` is 0. A silently wrong value whose
# answer depends on the receiver's size (the compatibility policy, plan rc2-sweep-fixes Y6b,
# 2026-10-18; left so in Y2).
# expected-from: SabiRuby (CRuby 3.2 raises TypeError for &, | and ^, in its own words, and
# answers allbits?/anybits?/nobits? with false, false, true).
# The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

t("(2**70) & 1.5") { (2**70) & 1.5 }
t("(2**70) | 1.5") { (2**70) | 1.5 }
t("(2**70) ^ 1.5") { (2**70) ^ 1.5 }
t("(2**70).allbits?(1.5)") { (2**70).allbits?(1.5) }
t("(2**70).anybits?(1.5)") { (2**70).anybits?(1.5) }
t("(2**70).nobits?(1.5)") { (2**70).nobits?(1.5) }
t("7 & 1.5") { 7 & 1.5 }
t("(2**70) & 3") { (2**70) & 3 }
