# Four more differences from mruby 4.1.0-rc2, found while doing L1-L5
# (docs/plans/rc2-leftovers2-plan.md, M1-M4). Expectation: mruby 4.1.0-rc2 (.rc.out).
# M1: `X::Y` and `Module#const_get` are `mrb_const_get` → `const_get(..., skip = FALSE)`
# (src/variable.c), which walks the class and its ancestors but stops at Object, so a class
# does not answer a top-level constant (`Foo::String` is a NameError, `Object::String` is
# not); `defined?(Foo::String)` is a different path and still says "constant". M2: something
# that is not a class or module before `::` is `mod_const_check`'s TypeError, and before
# `class`/`module` `check_if_class_or_module`'s. M3: the ends of a Range given to `Array#[]=`
# go through `mrb_as_int` (`mrb_range_beg_len`, src/range.c), so a Float end is truncated.
# M4: an index of `Array#[]=` that is not a number says what `mrb_as_int` says.
def msg; p yield; rescue Exception => e; p [e.class, e.message]; end

# M1
class Foo; end
module Mo; end
class Sub < Foo; end
class Bo < BasicObject; end
msg { Foo::String }
msg { Sub::String }
msg { Bo::String }
msg { Object::String }
msg { Foo.singleton_class::String }
msg { Foo.const_get(:String) }
msg { Foo.const_get("String") }
msg { Foo.const_get("Object::String") }
msg { Object.const_get("Foo::String") }
msg { Object.const_get("Foo::Object::String") }
msg { Foo.const_get(:Object) }
msg { Foo::Object }
msg { Foo.const_get("Kernel") }
msg { Mo::String }
msg { Comparable::Kernel }
msg { Object.const_get("Comparable::String") }
msg { Object.const_get("Math::PI") }
msg { defined?(Foo::String) }
msg { defined?(Sub::String) }
msg { defined?(Mo::String) }
msg { Foo.const_defined?(:String) }
class Foo; X = 1; end
msg { Sub::X }
msg { Sub.const_get(:X) }
module Inc; IX = 2; end
class Foo; include Inc; end
msg { Sub::IX }
class Hook; def self.const_missing(n); [:cm, n]; end; end
msg { Hook::String }
msg { Hook.const_get(:String) }

# M2
x = 1
msg { 1::X }
msg { nil::X }
msg { "s"::X }
msg { x::Y = 2 }
msg { class x::Foo; end }
msg { module x::Mm; end }
NotAMod = [1]
msg { Object.const_get("NotAMod::X") }
msg { Object.const_get("NotAMod") }

# M3
msg { a = [1, 2, 3, 4, 5]; a[(1.0..2.0)] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[(1.5..2.7)] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[(1..2.0)] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[(1.0..)] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[(1.0...3.0)] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[(-2.5..-1.0)] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[("a".."b")] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[1.0, 2] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[1, 2.0] = 9; a }
msg { a = [1, 2, 3, 4, 5]; a[1.9] = 9; a }

# M4
msg { a = [1]; a[nil] = 1 }
msg { a = [1]; a["x"] = 1 }
msg { a = [1]; a[:s] = 1 }
msg { a = [1]; a[nil, 1] = 1 }
msg { a = [1]; a[Object.new] = 1 }
msg { a = [1]; a[true] = 1 }
msg { a = [1]; a[(nil..nil)] = 1; a }
