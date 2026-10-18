# A copy of an exception keeps its message and backtrace (rc2-sweep cause `exc-copy`). In mruby
# 4.1.0-rc2 an exception keeps them in `struct RException` (`mesg`, `backtrace`), and
# `init_copy` (src/class.c:4349, the MRB_TT_EXCEPTION arm at :4366) copies only the instance
# variables, so `dup` and `clone` lose both: the copy's message is the class name and its
# backtrace nil. `Exception#exception(msg)` (`exc_exception`, src/error.c:87) is a `clone` with
# the new message, so it loses the backtrace. A value that silently loses its contents is a
# defect under the compatibility policy, so SabiRuby copies them (found in plan rc2-sweep-fixes
# Y4, C by the policy, 2026-10-18).
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
def raised
  raise "x"
rescue => e
  e
end
t("new('a').dup.message") { RuntimeError.new("a").dup.message }
t("new('a').clone.message") { RuntimeError.new("a").clone.message }
t("raised.dup.backtrace") { raised.dup.backtrace.class }
t("raised.clone.backtrace") { raised.clone.backtrace.class }
t("raised.exception('b').backtrace") { raised.exception("b").backtrace.class }
# what rc2 keeps: the new message, and the receiver itself without an argument
t("raised.exception('b').message") { raised.exception("b").message }
t("raised.exception.equal?") { e = raised; e.exception.equal?(e) }
