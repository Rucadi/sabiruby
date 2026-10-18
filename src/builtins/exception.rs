//! Exception hierarchy natives (the classes themselves are created in `Vm::new`).

use alloc::{format, string::String, vec, vec::Vec};

use crate::error::VmResult;
use crate::object::{Aspec, ObjKind};
use crate::value::Value;
use crate::vm::Vm;

fn exc_to_s(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    let o = s.obj().unwrap();
    let m = vm.heap.ivar_get(o, vm.s.mesg);
    if m.is_nil() { let c = vm.real_class_of(s); let n = vm.class_name(c); return Ok(vm.str_from(n)); }
    if vm.str_bytes(m).is_some() { return Ok(m); }
    let b = vm.as_string(m)?; // non-String message (e.g. a Symbol) is converted
    Ok(vm.str_new(&b))
}

/// `Exception#backtrace` (`exc_backtrace`): the text of what `raise` recorded, or what
/// `set_backtrace` was given. An exception that was never raised answers nil.
fn exc_backtrace(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    let o = s.obj().unwrap();
    let ks = vm.intern("@__btstr");
    if vm.heap.get(o).ivars.iter().any(|(n, _)| *n == ks) { return Ok(vm.heap.ivar_get(o, ks)); }
    let k = vm.intern("@__bt");
    let flat: Vec<i64> = match vm.heap.ivar_get(o, k).obj().map(|a| &vm.heap.get(a).kind) {
        Some(ObjKind::Array(v)) => v.iter().filter_map(|x| match x.get() { Value::Int(i) => Some(i), _ => None }).collect(),
        _ => {
            let r = vm.intern("@__raised");
            return if vm.heap.ivar_get(o, r).truthy() { Ok(vm.ary_new(vec![])) } else { Ok(Value::Nil) };
        }
    };
    let text: Vec<Value> = vm.backtrace_text(&flat).into_iter().map(|t| vm.str_from(t)).collect();
    Ok(vm.ary_new(text))
}

pub fn init(vm: &mut Vm) {
    let c = vm.core;
    vm.define_methods_args(c.exception, &[
        ("initialize", Aspec::opt(1), |vm, s, a, _b| { if let Some(m) = a.first() { let mesg = vm.s.mesg; vm.heap.ivar_set(s.obj().unwrap(), mesg, *m); } Ok(s) }),
        // `exc_exception`: the receiver itself for no argument or for itself (`mrb_obj_equal`);
        // Exception has no `==` of its own to make a copy look the same
        ("exception", Aspec::opt(1), |vm, s, a, _b| { if a.is_empty() || a[0] == s { return Ok(s); } let d = super::object_dup(vm, s)?; let mesg = vm.s.mesg; vm.heap.ivar_set(d.obj().unwrap(), mesg, a[0]); Ok(d) }),
        ("to_s", Aspec::none(), exc_to_s),
        ("message", Aspec::none(), |vm, s, _a, _b| { let to_s = vm.s.to_s; vm.funcall(s, to_s, &[], Value::Nil) }),
        ("inspect", Aspec::none(), |vm, s, _a, _b| { let c = vm.real_class_of(s); let cn = vm.class_name(c); let m = vm.heap.ivar_get(s.obj().unwrap(), vm.s.mesg); let mb = if m.is_nil() { Vec::new() } else { vm.as_string(m)? }; if mb.is_empty() { return Ok(vm.str_from(cn)); } let ms = String::from_utf8_lossy(&mb).into_owned(); Ok(vm.str_from(format!("#<{cn}: {ms}>"))) }),
        ("backtrace", Aspec::none(), exc_backtrace),
        ("set_backtrace", Aspec::req(1), |vm, s, a, _b| {
            // what the program gives is kept as it stands (`exc_set_backtrace`), once
            // `set_backtrace` (src/error.c) has found it an Array (`mrb_array_p`: not a Struct)
            // of Strings
            let ok = match vm.plain_ary(a[0]) { Some(l) => l.iter().all(|x| vm.str_bytes(x.get()).is_some()), None => false };
            if !ok { return Err(vm.raise_type("backtrace must be Array of String")); }
            let k = vm.intern("@__btstr");
            vm.heap.ivar_set(s.obj().unwrap(), k, a[0]);
            Ok(a[0])
        }),
        ("full_message", Aspec::any(), |vm, s, _a, _b| { let insp = vm.inspect_str(s)?; Ok(vm.str_from(insp)) }),
    ]);
    // `MRB_MT_PRIVATE` in `exc_rom_entries` (src/error.c)
    vm.mark_private(c.exception, &["initialize"]);
    // Exception.exception(msg) == Exception.new(msg)
    let sc = vm.singleton_class(Value::Obj(c.exception)).unwrap();
    vm.define_method_args(sc, "exception", Aspec::opt(1), |vm, s, a, b| vm.class_new_instance(s.obj().unwrap(), a, b));
}
