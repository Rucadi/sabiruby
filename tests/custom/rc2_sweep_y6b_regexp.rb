# regexp-only: String#match and match? with a position past the end
# The Regexp half of rc2_sweep_y6b.rb (docs/plans/rc2-sweep-fixes-plan.md Y6b):
# `match-pos-past-end`. rc2's `re_char_to_byte` refuses a position past the last character
# (`mrb_str_char_to_byte` counts one byte past the end for it), so `match` answers nil and
# `match?` false there; a position at the end still matches the empty pattern.
# expected-from: the reference (.expected is .rc.out).
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

t("''.match('', 1)") { "".match("", 1) }
t("''.match?('', 1)") { "".match?("", 1) }
t("'ab'.match('', 2)") { "ab".match("", 2) }
t("'ab'.match('', 3)") { "ab".match("", 3) }
t("'ab'.match?('', 2)") { "ab".match?("", 2) }
t("'ab'.match?('', 3)") { "ab".match?("", 3) }
t("'ab'.match(//, -2)") { "ab".match(//, -2) }
t("'ab'.match(//, -3)") { "ab".match(//, -3) }
t("/b/.match('ab', 1)") { /b/.match("ab", 1) }
t("/b/.match('ab', 3)") { /b/.match("ab", 3) }
t("/b/.match?('ab', 3)") { /b/.match?("ab", 3) }
t("''.method(:sub)") { m = "".method(:sub); [m.arity, m.parameters] }
