/* What the reference VM itself keeps for a method: the MRB_ARGS_* declaration it checks the
 * argument count against (src/vm.c check_argument_count) and builds Method#arity from
 * (src/proc.c mrb_proc_arity). Read back through the same macros the VM uses, so the table
 * tools/rc2_aspec.sh writes is the reference's own values, not a reading of its sources.
 * Built only inside a throwaway container by tools/rc2_aspec.sh. */
#include <mruby.h>
#include <mruby/class.h>
#include <mruby/proc.h>
#include <mruby/array.h>
#include <mruby/irep.h>
#include <mruby/opcode.h>

/* __aspec(mod, name) -> [kind, aspec]
 *   kind "func":  a C function entry (MRB_MT_ENTRY / mrb_define_method); aspec is MRB_MT_ASPEC
 *   kind "cproc": a C function behind an RProc; aspec is the decompressed caspec, or -1 when
 *                 the proc carries none (no check, arity -1), 0 when it is MRB_PROC_NOARG
 *   kind "ruby":  a method written in Ruby; aspec is the operand of the OP_ENTER it starts
 *                 with (the same bit layout, checked by vm_op_enter), or nil without one
 *   nil when the name is undefined there */
static mrb_value
aspec_of(mrb_state *mrb, mrb_value self)
{
  mrb_value mod;
  mrb_sym name;
  mrb_get_args(mrb, "Cn", &mod, &name);
  struct RClass *c = mrb_class_ptr(mod);
  mrb_method_t m = mrb_method_search_vm(mrb, &c, name);
  if (MRB_METHOD_UNDEF_P(m)) return mrb_nil_value();
  mrb_value r[2];
  if (MRB_METHOD_FUNC_P(m)) {
    r[0] = mrb_str_new_lit(mrb, "func");
    r[1] = mrb_int_value(mrb, (mrb_int)MRB_MT_ASPEC(m.flags));
    return mrb_ary_new_from_values(mrb, 2, r);
  }
  const struct RProc *p = MRB_METHOD_PROC(m);
  while (p && MRB_PROC_ALIAS_P(p)) p = p->upper;
  if (p && MRB_PROC_CFUNC_P(p)) {
    uint32_t bits = p->flags & MRB_PROC_CASPEC_MASK;
    r[0] = mrb_str_new_lit(mrb, "cproc");
    if (bits != 0) r[1] = mrb_int_value(mrb, (mrb_int)mrb_proc_decompress_caspec(bits));
    else if (MRB_PROC_NOARG_P(p)) r[1] = mrb_int_value(mrb, 0);
    else r[1] = mrb_int_value(mrb, -1);
    return mrb_ary_new_from_values(mrb, 2, r);
  }
  r[0] = mrb_str_new_lit(mrb, "ruby");
  r[1] = mrb_nil_value();
  if (p && p->body.irep && p->body.irep->ilen > 0 && p->body.irep->iseq[0] == OP_ENTER) {
    const mrb_code *i = p->body.irep->iseq;
    r[1] = mrb_int_value(mrb, (mrb_int)(((uint32_t)i[1] << 16) | ((uint32_t)i[2] << 8) | i[3]));
  }
  return mrb_ary_new_from_values(mrb, 2, r);
}

void
mrb_mruby_aspec_dump_gem_init(mrb_state *mrb)
{
  mrb_define_method_id(mrb, mrb->kernel_module, mrb_intern_lit(mrb, "__aspec"), aspec_of, MRB_ARGS_REQ(2));
}

void
mrb_mruby_aspec_dump_gem_final(mrb_state *mrb)
{
}
