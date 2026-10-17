# regexp-only: Regexp#match and String#gsub
# The Regexp half of rc2_sweep_y2.rb (docs/plans/rc2-sweep-fixes-plan.md Y2): `regexp-match-nil`
# and `conv-order` (rc2's `regexp_match` reads "o|i&", so the position is converted before a
# nil subject answers nil), and `enum-args-kw` (the keyword Hash given to `gsub` is an argument
# of the Enumerator, as rc2's C call to `to_enum` hands it on).
# expected-from: the reference (.expected is .rc.out).
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

t("match nil str") { /x/.match(nil, "") }
t("match nil 1") { /x/.match(nil, 1) }
t("match true wide") { /x/.match(true, 2**70) }
t("match nan") { /x/.match("x", Float::NAN) }
t("gsub kw") { "abc".gsub(k: 1) }
t("gsub! kw") { (+"abc").gsub!(k: 1) }
t("gsub hash") { "abc".gsub({k: 1}) }
