# `const_get("")` raises NameError, as `const_get(:"")` does (rc2-sweep cause `const-get-empty`).
# In mruby 4.1.0-rc2 `mrb_mod_const_get` (src/class.c:4023) looks a Symbol up at once, but walks
# a String path segment by segment (`while (off < len)`, :4039): for an empty String the loop never
# runs and the receiver itself is answered, so `Object.const_get("")` is Object. The same question
# answers two ways by how the name is written, and silently (the compatibility policy, plan
# rc2-sweep-fixes Y6b, 2026-10-18).
# expected-from: SabiRuby (CRuby 3.2 raises NameError "wrong constant name " too).
# The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

c = Class.new
t("const_get('')") { c.const_get("").equal?(c) }
t("const_get(:'')") { c.const_get(:"") }
t("Object.const_get('')") { Object.const_get("") }
t("Object.const_get('String')") { Object.const_get("String") }
