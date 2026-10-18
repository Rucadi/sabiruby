//! Range (`each`, `to_a` etc. come from mrblib; the primitives are here).

use alloc::{vec, vec::Vec};

use crate::argc;
use crate::error::VmResult;
use crate::object::{Aspec, ObjKind};
use crate::value::{Slot, Value};
use crate::vm::Vm;

fn parts(vm: &Vm, v: Value) -> (Value, Value, bool) {
    match v.obj().map(|o| &vm.heap.get(o).kind) { Some(ObjKind::Range { begin, end, excl }) => (begin.get(), end.get(), *excl), _ => (Value::Nil, Value::Nil, false) }
}

pub fn range_inspect(vm: &mut Vm, v: Value, insp: bool) -> VmResult<Vec<u8>> {
    let (b, e, x) = parts(vm, v);
    let mut out = if b.is_nil() && insp { vec![] } else if insp { vm.inspect(b)? } else { vm.as_string(b)? };
    out.extend_from_slice(if x { b"..." } else { b".." });
    if !(e.is_nil() && insp) { out.extend(if insp { vm.inspect(e)? } else { vm.as_string(e)? }); }
    Ok(out)
}

fn cover(vm: &mut Vm, s: Value, v: Value) -> VmResult<bool> {
    let (b, e, x) = parts(vm, s);
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

pub fn init(vm: &mut Vm) {
    let c = vm.core;
    // `Range.new` is `Class#new`: `instance_alloc` gives a plain object that `initialize` fills
    vm.define_methods_args(c.range, &[
        // private (set below); `dup`/`clone` call it on the fresh copy
        ("initialize_copy", Aspec::req(1), |vm, s, a, _b| { if s == a[0] { return Ok(s); } let cls = vm.real_class_of(s); if vm.real_class_of(a[0]) != cls { return Err(vm.raise_type("wrong argument class")); } if s.obj().map(|o| matches!(vm.heap.get(o).kind, ObjKind::Range { .. })).unwrap_or(false) { let n = vm.intern("initialize"); return Err(vm.name_error(n, "'initialize' called twice")); } let (b, e, x) = match a[0].obj().map(|o| &vm.heap.get(o).kind) { Some(ObjKind::Range { begin, end, excl }) => (*begin, *end, *excl), _ => return Err(vm.raise_type("wrong argument class")) }; if let Some(o) = s.obj() { vm.heap.get_mut(o).kind = ObjKind::Range { begin: b, end: e, excl: x }; } Ok(s) }),
        ("initialize", Aspec::any(), |vm, s, a, _b| { argc!(vm, a, 2, 3); vm.check_range_ends(a[0], a[1])?; if s.obj().map(|o| matches!(vm.heap.get(o).kind, ObjKind::Range { .. })).unwrap_or(false) { let n = vm.intern("initialize"); return Err(vm.name_error(n, "'initialize' called twice")); } let excl = a.len() == 3 && a[2].truthy(); if let Some(o) = s.obj() { vm.heap.get_mut(o).kind = ObjKind::Range { begin: Slot::from(a[0]), end: Slot::from(a[1]), excl }; } Ok(s) }),

        ("begin", Aspec::none(), |vm, s, _a, _b| Ok(parts(vm, s).0)),
        ("end", Aspec::none(), |vm, s, _a, _b| Ok(parts(vm, s).1)),
        ("exclude_end?", Aspec::none(), |vm, s, _a, _b| Ok(Value::bool(parts(vm, s).2))),
        ("==", Aspec::req(1), |vm, s, a, _b| { if s == a[0] { return Ok(Value::True); } if !matches!(a[0].obj().map(|o| &vm.heap.get(o).kind), Some(ObjKind::Range { .. })) { return Ok(Value::False); } let (b1, e1, x1) = parts(vm, s); let (b2, e2, x2) = parts(vm, a[0]); Ok(Value::bool(x1 == x2 && vm.equal(b1, b2)? && vm.equal(e1, e2)?)) }),
        ("eql?", Aspec::req(1), |vm, s, a, _b| { let (b1, e1, x1) = parts(vm, s); let (b2, e2, x2) = parts(vm, a[0]); Ok(Value::bool(matches!(a[0].obj().map(|o| &vm.heap.get(o).kind), Some(ObjKind::Range { .. })) && x1 == x2 && vm.eql(b1, b2) && vm.eql(e1, e2))) }),
        ("===", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(cover(vm, s, a[0])?)) }),
        ("include?", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(cover(vm, s, a[0])?)) }),
        ("member?", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(cover(vm, s, a[0])?)) }),
        ("to_s", Aspec::none(), |vm, s, _a, _b| { let b = range_inspect(vm, s, false)?; Ok(vm.str_new(&b)) }),
        ("inspect", Aspec::none(), |vm, s, _a, _b| { let b = range_inspect(vm, s, true)?; Ok(vm.str_new(&b)) }),
        ("__num_to_a", Aspec::none(), |vm, s, _a, _b| { let (b, e, x) = parts(vm, s); match (b, e) { (Value::Int(b), Value::Int(e)) => { let last = if x { e - 1 } else { e }; let v: Vec<Value> = (b..=last).map(Value::Int).collect(); Ok(vm.ary_new(v)) } (Value::Int(_), Value::Nil) => Err(vm.raise(vm.core.range_error, "cannot convert endless range to an array")), _ => Ok(Value::Nil) } }),
    ]);
    // `MRB_MT_PRIVATE` in `range_rom_entries` (src/range.c) for both
    vm.mark_private(c.range, &["initialize", "initialize_copy"]);
}
