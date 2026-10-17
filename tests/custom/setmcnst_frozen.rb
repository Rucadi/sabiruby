# `A::Y = 2` (OP_SETMCNST) on a frozen class or module (rc2-sweep cause `setmcnst-frozen`):
# SabiRuby assigned. rc2's OP_SETMCNST calls `mrb_const_set`, whose `mrb_obj_iv_set` checks the
# module with `mrb_check_frozen` (src/variable.c): FrozenError "can't modify frozen %T", as
# OP_SETCONST in a class body does. The book's program is data/code/exp_GETCONST_frozen.rb
# (opcodes.re, SETMCNST).
# expected-from: the reference (.expected is .rc.out).
def t
  p yield
rescue Exception => e
  p [e.class, e.message]
end
class A; end
module M; end
A.freeze
M.freeze
t { A::Y = 2 }
t { A.const_defined?(:Y) }
t { M::Y = 2 }
t { class A; X = 1; end }
t { A.singleton_class::Y = 3 }
class B; end
t { B::Y = 2 }
t { 3::X = 1 }
