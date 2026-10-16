# `a[i, len] = v` with a length that is not a number. A deviation from mruby 4.1.0-rc2 taken
# under docs/design/compatibility.md ("inconsistent within mruby itself"; README.md, "Known
# deviations"). rc2's `mrb_ary_aset` (src/array.c) reads the length with `aget_index`, whose
# fallback `mrb_get_args(mrb, "i*!", &i, ...)` converts the METHOD'S FIRST ARGUMENT, not the
# value it was handed: the index is converted a second time and used as the length, and the
# length itself is never looked at. Master 158760853 still does it.
# rc2 answers (.rc.out): [1]; a[0, nil] = 1 -> [1, 1]; [1, 2, 3]; a[1, nil] = 9 -> [1, 9, 3];
# a[0, "x"] = 1 -> [1, 1]; a[1.5, nil] = 1 -> [1, 1, 3]; a[nil, 1] and a[nil, nil] raise.
# SabiRuby converts the length itself through `mrb_as_int` (TypeError, "nil cannot be
# converted to Integer"), as `Array#[]` and `String#[]=` do with theirs.
# expected-from: CRuby 3.2.6 and 3.4 (ruby:3.4); every line raises TypeError there. Only the
# class is printed, since the wording is CRuby's own ("no implicit conversion from nil to
# integer") where SabiRuby keeps mruby's.
def t
  p yield
rescue Exception => e
  p e.class
end
t { a = [1]; a[0, nil] = 1; a }
t { a = [1, 2, 3]; a[1, nil] = 9; a }
t { a = [1]; a[0, "x"] = 1; a }
t { a = [1, 2, 3]; a[1.5, nil] = 1; a }
t { a = [1]; a[nil, 1] = 1; a }
t { a = [1]; a[nil, nil] = 1; a }
# the array is left as it was
a = [1, 2, 3]
t { a[1, nil] = 9 }
p a
