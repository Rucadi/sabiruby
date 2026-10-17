# The "given" count of a wrong number of arguments when keywords are involved. mruby 4.1.0-rc2
# counts it from the positional arguments (`ci->n`) and adds one when there are none and the
# keyword Hash is not empty (`argnum_error`, src/vm.c:2154-2170); the check itself counts
# differently (the fast path compares `argc + ci->kw`, a method that takes keywords does not
# take the Hash as positional). Four calls then read "given 1, expected 1", contradicting
# themselves. That is inconsistent within mruby itself, so SabiRuby keeps the number the
# method was given, which is what the check counted (the author's decision of 2026-10-17,
# `docs/plans/rc2-leftovers10-plan.md`, W2).
# expected-from: SabiRuby (CRuby 3.2 says the same numbers). The reference's answers are in
# .rc.out. README, "Known deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end
def f1(a); end
def f5(a, k: 1); end
def f6(a, b = 1, k: 1); end
def f8(a, **kw); end
def f2(a, b = nil); end
t("f1(1, k: 1)") { f1(1, k: 1) }
t("f5(k: 2)") { f5(k: 2) }
t("f6(k: 1)") { f6(k: 1) }
t("f8(k: 1)") { f8(k: 1) }
# the same calls without keywords, and one the reference counts the same way as SabiRuby
t("f1(1, 2)") { f1(1, 2) }
t("f5()") { f5 }
t("f2(1, 2, k: 3)") { f2(1, 2, k: 3) }
