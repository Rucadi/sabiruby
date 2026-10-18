//! Range (`each`, `to_a` etc. come from mrblib; the primitives are here).

use alloc::{vec, vec::Vec};

use crate::argc;
use crate::error::VmResult;
use crate::object::{Aspec, ObjKind};
use crate::value::{Slot, Value};
use crate::vm::Vm;

/// `mrb_range_ptr` (src/range.c): the three parts, or ArgumentError "uninitialized range" for
/// a Range `initialize` has not filled (`Range.allocate`; the reference's RANGE_INITIALIZED).
/// Every Range method that reads the parts asks this first, as every one of rc2's does.
pub(crate) fn parts(vm: &mut Vm, v: Value) -> VmResult<(Value, Value, bool)> {
    match v.obj().map(|o| &vm.heap.get(o).kind) {
        Some(ObjKind::Range { begin, end, excl }) => Ok((begin.get(), end.get(), *excl)),
        _ => Err(uninitialized(vm)),
    }
}

#[cold]
#[inline(never)]
fn uninitialized(vm: &mut Vm) -> crate::error::VmError {
    vm.raise_arg("uninitialized range")
}

pub fn range_inspect(vm: &mut Vm, v: Value, insp: bool) -> VmResult<Vec<u8>> {
    let (b, e, x) = parts(vm, v)?;
    let mut out = if b.is_nil() && insp { vec![] } else if insp { vm.inspect(b)? } else { vm.as_string(b)? };
    out.extend_from_slice(if x { b"..." } else { b".." });
    if !(e.is_nil() && insp) { out.extend(if insp { vm.inspect(e)? } else { vm.as_string(e)? }); }
    Ok(out)
}

fn cover(vm: &mut Vm, s: Value, v: Value) -> VmResult<bool> {
    let (b, e, x) = parts(vm, s)?;
    let cmp = vm.intern("<=>");
    if !b.is_nil() {
        let r = vm.funcall(b, cmp, &[v], Value::Nil)?;
        match r { Value::Int(i) if i <= 0 => {} _ => return Ok(false) }
    }
    if !e.is_nil() {
        let r = vm.funcall(v, cmp, &[e], Value::Nil)?;
        match r { Value::Int(i) if i < 0 || (i == 0 && !x) => {} _ => return Ok(false) }
    }
    Ok(true)
}

/// `MRB_TT_RANGE`: a Range object, filled in or not (an unfilled one is a plain object of a
/// Range class here, `instance_alloc`).
pub(crate) fn is_range_obj(vm: &Vm, v: Value) -> bool {
    match v.obj() {
        Some(o) => matches!(vm.heap.get(o).kind, ObjKind::Range { .. }) || vm.obj_is_kind_of(v, vm.core.range),
        None => false,
    }
}

pub fn init(vm: &mut Vm) {
    let c = vm.core;
    // `Range.new` is `Class#new`: `instance_alloc` gives a plain object that `initialize` fills
    vm.define_methods_args(c.range, &[
        // private (set below); `dup`/`clone` call it on the fresh copy
        // `range_initialize_copy`: the source's parts (`mrb_range_ptr`), then `range_ptr_init`'s
        // checks on the copy (`r_check`, "'initialize' called twice")
        ("initialize_copy", Aspec::req(1), |vm, s, a, _b| { if s == a[0] { return Ok(s); } let cls = vm.real_class_of(s); if vm.real_class_of(a[0]) != cls { return Err(vm.raise_type("wrong argument class")); } let (b, e, x) = parts(vm, a[0])?; vm.check_range_ends(b, e)?; if s.obj().map(|o| matches!(vm.heap.get(o).kind, ObjKind::Range { .. })).unwrap_or(false) { let n = vm.intern("initialize"); return Err(vm.name_error(n, "'initialize' called twice")); } if let Some(o) = s.obj() { vm.heap.get_mut(o).kind = ObjKind::Range { begin: Slot::from(b), end: Slot::from(e), excl: x }; } Ok(s) }),
        ("initialize", Aspec::any(), |vm, s, a, _b| { argc!(vm, a, 2, 3); vm.check_range_ends(a[0], a[1])?; if s.obj().map(|o| matches!(vm.heap.get(o).kind, ObjKind::Range { .. })).unwrap_or(false) { let n = vm.intern("initialize"); return Err(vm.name_error(n, "'initialize' called twice")); } let excl = a.len() == 3 && a[2].truthy(); if let Some(o) = s.obj() { vm.heap.get_mut(o).kind = ObjKind::Range { begin: Slot::from(a[0]), end: Slot::from(a[1]), excl }; } Ok(s) }),

        ("begin", Aspec::none(), |vm, s, _a, _b| Ok(parts(vm, s)?.0)),
        ("end", Aspec::none(), |vm, s, _a, _b| Ok(parts(vm, s)?.1)),
        ("exclude_end?", Aspec::none(), |vm, s, _a, _b| Ok(Value::bool(parts(vm, s)?.2))),
        // `range_eq`: the same object, then an instance of the receiver's own class (`mrb_obj_is_instance_of`)
        ("==", Aspec::req(1), |vm, s, a, _b| { if s == a[0] { return Ok(Value::True); } if !is_range_obj(vm, a[0]) || vm.real_class_of(a[0]) != vm.real_class_of(s) { return Ok(Value::False); } let (b1, e1, x1) = parts(vm, s)?; let (b2, e2, x2) = parts(vm, a[0])?; Ok(Value::bool(vm.equal(b1, b2)? && vm.equal(e1, e2)? && x1 == x2)) }),
        // `range_eql`: the same object, then any Range (`mrb_range_p`)
        ("eql?", Aspec::req(1), |vm, s, a, _b| { if s == a[0] { return Ok(Value::True); } if !is_range_obj(vm, a[0]) { return Ok(Value::False); } let (b1, e1, x1) = parts(vm, s)?; let (b2, e2, x2) = parts(vm, a[0])?; Ok(Value::bool(vm.eql(b1, b2) && vm.eql(e1, e2) && x1 == x2)) }),
        ("===", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(cover(vm, s, a[0])?)) }),
        ("include?", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(cover(vm, s, a[0])?)) }),
        ("member?", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(cover(vm, s, a[0])?)) }),
        ("to_s", Aspec::none(), |vm, s, _a, _b| { let b = range_inspect(vm, s, false)?; Ok(vm.str_new(&b)) }),
        ("inspect", Aspec::none(), |vm, s, _a, _b| { let b = range_inspect(vm, s, true)?; Ok(vm.str_new(&b)) }),
        ("__num_to_a", Aspec::none(), |vm, s, _a, _b| { let (b, e, x) = parts(vm, s)?; match (b, e) { (Value::Int(b), Value::Int(e)) => { let last = if x { e - 1 } else { e }; let v: Vec<Value> = (b..=last).map(Value::Int).collect(); Ok(vm.ary_new(v)) } (Value::Int(_), Value::Nil) => Err(vm.raise(vm.core.range_error, "cannot convert endless range to an array")), _ => Ok(Value::Nil) } }),
    ]);
    // `MRB_MT_PRIVATE` in `range_rom_entries` (src/range.c) for both
    vm.mark_private(c.range, &["initialize", "initialize_copy"]);
}
