# `private :m` (and `public`, `protected`) in a class with a prepended module changes the
# visibility of the class's own `m` and leaves the module's alone (rc2-sweep row
# `vis-prepended-copy`, found in rc2-sweep-fixes Y8a). In mruby 4.1.0-rc2 `mrb_mod_visibility`
# (src/class.c:2482) looks the method up from the class (`mrb_method_search(mrb, c, mid)`, :2523),
# which finds the prepended module's `P#m` first, and writes that copy into the class's origin
# (`MRB_CLASS_ORIGIN(t)`, :2518), the table that holds `C#m`: `C#m` is replaced by a private
# `P#m`, whose `super` then finds nothing ("no superclass method 'm'"). Changing a method's
# visibility silently replaces another method (the compatibility policy, the author's decision of
# 2026-10-19, plan release-0.8 R1). SabiRuby (`Vm::set_visibility`, src/vm.rs) changes the flag of
# the entry already in the origin.
# expected-from: SabiRuby (CRuby 3.2 answers every line the same, but for the wording of the
# private-method NoMethodError).
# The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

module P
  def m; "P#m+" + super; end
end
class C
  def m; "C#m"; end
  prepend P
  private :m
end
t("C.new.m") { C.new.m }
t("C.new.send(:m)") { C.new.send(:m) }

# the same with protected, then public again
module P2
  def m; "P2#m+" + super; end
end
class C2
  def m; "C2#m"; end
  prepend P2
  protected :m
  public :m
end
t("C2.new.m") { C2.new.m }

# without a prepended module both answer alike
class D
  def m; "D#m"; end
  private :m
end
t("D.new.m") { D.new.m }
t("D.new.send(:m)") { D.new.send(:m) }
