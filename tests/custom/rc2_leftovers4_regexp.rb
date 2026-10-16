# regexp-only: MatchData and a Regexp index
# `MatchData#[]` (mruby-regexp) reads its index and length with `mrb_as_int`, as rc2's
# `mrb_get_args("i")` does, and `String#[]=` with a Regexp that does not match says "regexp not
# matched" (docs/plans/rc2-leftovers4-plan.md, P2). Expectation: mruby 4.1.0-rc2 (.rc.out).
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue => e
  puts "#{label} => #{e.class}: #{e.message}"
end
t("regexp not matched") { s = "abcde"; s[/z/] = "X"; s }
m = /(a)(b)(c)/.match("abc")
[nil, "1", Float::NAN, 1.5].each do |x|
  t("md[#{x.inspect}]") { m[x] }
  t("md[#{x.inspect}, 1]") { m[x, 1] }
  t("md[0, #{x.inspect}]") { m[0, x] }
end
