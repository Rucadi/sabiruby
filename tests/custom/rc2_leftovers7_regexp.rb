# regexp-only: Regexp.union
# The String argument of `Regexp.union` (docs/plans/rc2-leftovers7-plan.md, T1; the rest is
# rc2_leftovers7.rb). Expectation: mruby 4.1.0-rc2 (.rc.out): `mrb_ensure_string_type`'s
# "%Y cannot be converted to String", without the name of the argument.
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue Exception => e
  puts "#{label} !! #{e.class}: #{e.message}"
end
class Named; end
[1, nil, :a, Named.new, 1.5].each do |v|
  d = v.is_a?(Named) ? "Named" : v.inspect
  t("Regexp.union #{d}") { Regexp.union(v) }
  t("Regexp.union('a', #{d})") { Regexp.union("a", v) }
  t("Regexp.escape #{d}") { Regexp.escape(v) }
end
