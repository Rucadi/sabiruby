# Too many arguments given through a splat to a native method that counts them itself
# (rc2-sweep cause `argc-splat`). mruby 4.1.0-rc2 declares `String#chomp`, `chomp!`, `to_i` and
# `Module#alias_method` MRB_ARGS_ANY (src/string.c:4473, 4474, 4495; src/class.c:4679), so the VM
# lets any count through and `mrb_get_args` counts. Its fast path (`get_args_fast`,
# src/class.c:1387) checks the count first, but it gives up on a call whose arguments came
# through a splat (they are packed, `ci->n == 15`), and the slow path (`get_args_v`) converts
# each argument as it reads it and counts only at the end (src/class.c:1913): `"abc".chomp(nil,
# nil)` is ArgumentError, `"abc".chomp(*[nil, nil])` TypeError. The same call answering two ways
# by how its arguments are written is inconsistent within mruby itself, so SabiRuby counts first
# in both forms (the compatibility policy, plan rc2-sweep-fixes Y5, 2026-10-18).
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
t("chomp(nil, nil)") { "abc".chomp(nil, nil) }
t("chomp(*[nil, nil])") { "abc".chomp(*[nil, nil]) }
t("chomp!(*[nil, nil])") { "abc".chomp!(*[nil, nil]) }
t("to_i(*[nil, nil])") { "12".to_i(*[nil, nil]) }
t("alias_method(*[nil, nil, nil])") { Class.new.send(:alias_method, *[nil, nil, nil]) }
# the right count through a splat is the same on both
t("chomp(*['c'])") { "abc".chomp(*["c"]) }
t("to_i(*[16])") { "ff".to_i(*[16]) }
