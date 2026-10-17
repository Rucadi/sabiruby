# `Integer#infinite?` (and that of the other Numerics that are not a Float). mruby 4.1.0-rc2
# answers false there: `num_infinite_p` (src/numeric.c, the Numeric entry of
# `numeric_rom_entries`) is `return mrb_false_value()`, while `flo_infinite_p` answers nil for a
# finite Float, 1 or -1 for an infinity. The same question answers false or nil by the class of
# a finite number, which is inconsistent within mruby itself (master 891d1d9d is the same), so
# SabiRuby keeps nil for every finite number (the author's decision of 2026-10-17,
# `docs/plans/rc2-leftovers9-plan.md`, V2).
# expected-from: SabiRuby (nil; CRuby 3.2 agrees: `Numeric#infinite?` is nil). The reference's
# answers (false for Integer, a wide Integer and Rational) are in .rc.out. README, "Known
# deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end

t("1.infinite?") { 1.infinite? }
t("0.infinite?") { 0.infinite? }
t("-5.infinite?") { -5.infinite? }
t("(2**70).infinite?") { (2**70).infinite? }
t("Rational(1, 3).infinite?") { Rational(1, 3).infinite? }
t("1.5.infinite?") { 1.5.infinite? }
t("Float::INFINITY.infinite?") { Float::INFINITY.infinite? }
t("(-Float::INFINITY).infinite?") { (-Float::INFINITY).infinite? }
t("Float::NAN.infinite?") { Float::NAN.infinite? }
t("1.finite?") { 1.finite? }
t("Float::INFINITY.finite?") { Float::INFINITY.finite? }
t("1.infinite?(1)") { 1.infinite?(1) }
