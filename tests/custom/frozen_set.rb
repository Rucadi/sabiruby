# A frozen Set changes (rc2-sweep cause `frozen-set`). In mruby 4.1.0-rc2 the mutators of
# mruby-set (`set_add`, `set_delete`, `set_clear`, the `__merge` of `merge`, …,
# mrbgems/mruby-set/src/set.c) do not call `mrb_check_frozen_value`; only `reset` and `flatten!`
# do (set.c:1185, 1352). Every other collection (Array, Hash, String) refuses to change when
# frozen, so this is inconsistent within mruby itself and SabiRuby raises FrozenError (the
# compatibility policy, plan rc2-sweep-fixes Y5, 2026-10-18).
# expected-from: SabiRuby (CRuby 3.2 raises FrozenError too, in its own words).
# The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end
t("<< 2") { Set[1].freeze << 2 }
t("add(2)") { Set[1].freeze.add(2) }
t("delete(1)") { Set[1].freeze.delete(1) }
t("clear") { Set[1].freeze.clear }
t("merge([3])") { Set[1].freeze.merge([3]) }
t("still the same") { s = Set[1].freeze; begin; s << 2; rescue FrozenError; end; [s.frozen?, s.size] }
t("Array <<") { [1].freeze << 2 }
