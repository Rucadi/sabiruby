# The singleton class of an object that is not frozen, frozen by itself
# (`o.singleton_class.freeze`). mruby 4.1.0-rc2 lets `def o.x` and `define_method` through
# (`mt_writable` in src/class.c checks the object the singleton class is attached to, not the
# class, since ccdf75c1c of 2023, which added it to name the object in the FrozenError) but
# refuses `o.extend(M)` (`mrb_include_module` checks the singleton class itself): the same
# frozen table is writable through one door and not the other, an inconsistency within mruby.
# The author decided (2026-10-17, docs/plans/rc2-leftovers7-plan.md, T3) to keep SabiRuby's
# answer, a FrozenError for all of them, which is also CRuby's. (SabiRuby's `extend` looked
# only at the object until rc2-leftovers7 and went through; it now checks the singleton class,
# as rc2's `extend` does.)
# expected-from: SabiRuby (CRuby 3.2 agrees). The reference's answers are in .rc.out. README,
# "Known deviations".
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue Exception => e
  puts "#{label} !! #{e.class}"
end
o = Object.new
o.singleton_class.freeze
t("frozen?") { [o.frozen?, o.singleton_class.frozen?] }
t("def o.x") { def o.x; 1; end; o.x }
t("define_method") { o.singleton_class.send(:define_method, :y) { 2 }; o.y }
t("extend") { o.extend(Comparable); :extended }
t("ivar on the object") { o.instance_variable_set(:@a, 1) }
