# The union of an empty Set and an Array (rc2-sweep cause `set-uninit`). In mruby 4.1.0-rc2
# `Set#union` (`|`, `+`; mrbgems/mruby-set/mrblib/set.rb:117) is `__union(enum) ||
# dup.merge(enum)`: with an Array it takes the second branch, and `dup` of an empty Set is a Set
# without a table, because `set_init_copy` (mrbgems/mruby-set/src/set.c:233) copies with
# `kh_replace`, which leaves `data` NULL for an empty source (include/mruby/khash.h:393-399).
# `merge` then adds through `set_add`, which finds no table and raises RuntimeError
# "uninitialized Set" (`set_ensure_initialized`, set.c:158). `Set.new | [1]` refuses while
# `Set[0] | [1]` and `Set.new | Set[1]` answer: a refusal that depends on state rather than on
# the input is inconsistent within mruby itself (the compatibility policy, "mruby's limits are
# the specification"), so SabiRuby answers the union (plan rc2-sweep-fixes Y5, 2026-10-18).
# expected-from: SabiRuby (CRuby 3.2 answers the same sets). The reference's answers are in
# .rc.out. README, "Known deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end
t("Set.new | [1, 2]") { Set.new | [1, 2] }
t("Set.new.union([1, 2])") { Set.new.union([1, 2]) }
t("Set.new + [1, 2]") { Set.new + [1, 2] }
t("emptied by delete") { s = Set[1]; s.delete(1); s | [1, 2] }
t("Set.new.dup << 1") { Set.new.dup << 1 }
# the cases rc2 answers
t("Set[0] | [1]") { Set[0] | [1] }
t("Set.new | Set[1]") { Set.new | Set[1] }
