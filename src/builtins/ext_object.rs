//! mruby-object-ext (`mrbgems/mruby-object-ext/src/object.c`): `NilClass#to_a/to_h/to_i/to_f`,
//! `Kernel#itself`, `BasicObject#instance_exec`. Its Ruby part (`yield_self`/`then`, `tap`) is
//! `src/mrblib/object-ext.mrb`.

use alloc::vec;

use crate::object::Aspec;
use crate::value::Value;
use crate::vm::Vm;

pub fn init(vm: &mut Vm) {
    let c = vm.core;
    vm.define_methods_args(c.nil_class, &[
        ("to_a", Aspec::none(), |vm, _s, _a, _b| Ok(vm.ary_new(vec![]))),
        ("to_h", Aspec::none(), |vm, _s, _a, _b| Ok(vm.hash_new())),
        ("to_i", Aspec::none(), |_vm, _s, _a, _b| Ok(Value::Int(0))),
        ("to_f", Aspec::none(), |_vm, _s, _a, _b| Ok(Value::Float(0.0))),
    ]);
    vm.define_method_args(c.kernel, "itself", Aspec::none(), |_vm, s, _a, _b| Ok(s));
    // `mrb_object_exec(self, singleton_class(self))`: the block runs with `self` as self and
    // the singleton class as the definition target; the arguments go to the block.
    vm.define_method_args(c.basic_object, "instance_exec", Aspec::rest() | Aspec::block(), |vm, s, a, b| {
        if b.is_nil() { return Err(vm.raise_arg("no block given")); }
        // the caller's keywords stay keywords (the trailing Hash is the pending kdict)
        let kw = match (vm.pending_kw, a.last()) { (Some(k), Some(l)) if !k.is_nil() && k == *l => Some(k), _ => None };
        let pos = if kw.is_some() { &a[..a.len() - 1] } else { a };
        // called by a SEND the block becomes the frame (`mrb_object_exec` → `mrb_exec_irep`)
        vm.exec_block_with_self(b, s, pos, kw)
    });
}
