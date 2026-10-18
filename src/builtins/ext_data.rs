//! mruby-data (`mrbgems/mruby-data/src/data.c`): `Data.define`. Its Ruby part
//! (`__init_with_kw`) is `src/mrblib/data.mrb`. Instances are array-shaped like structs
//! (`ext_struct.rs`) and frozen once built.

use alloc::{format, vec::Vec};

use crate::argc;
use crate::error::VmResult;
use crate::object::{Aspec, ObjKind};
use crate::symbol::Sym;
use crate::value::{ObjId, Slot, Value};
use crate::vm::Vm;

use super::ext_struct::{ary_replace, class_new, corrupted, define_accessors, initialize_is, is_struct, s_members, values};
use super::object::{same_object, sym_arg};

fn data_class(vm: &mut Vm) -> ObjId {
    let n = vm.intern("Data");
    match vm.const_get(vm.core.object, n) { Some(Value::Obj(c)) => c, _ => vm.core.object }
}

fn class_members(vm: &mut Vm, c: ObjId) -> VmResult<Vec<Sym>> {
    let root = data_class(vm);
    s_members(vm, c, root, "data")
}

/// `data_members`: the members of a built instance
fn members(vm: &mut Vm, s: Value) -> VmResult<Vec<Sym>> {
    let len = values(vm, s).len();
    if !is_struct(vm, s) || len == 0 { return Err(corrupted(vm, "data")); }
    let c = vm.real_class_of(s);
    let m = class_members(vm, c)?;
    if len != m.len() { return Err(vm.raise_type(&format!("data size differs ({} required {} given)", m.len(), len))); }
    Ok(m)
}

fn s_members_m(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    let m = class_members(vm, s.obj().unwrap())?;
    Ok(vm.ary_new(m.into_iter().map(Value::Sym).collect()))
}

fn m_members(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    let c = vm.real_class_of(s);
    s_members_m(vm, Value::Obj(c), &[], Value::Nil)
}

/// `data_store`: the values, then frozen
fn store(vm: &mut Vm, data: Value, vals: &[Value]) {
    let o = data.obj().unwrap();
    if let ObjKind::Array(a) = &mut vm.heap.get_mut(o).kind { *a = vals.iter().map(|v| Slot::from(*v)).collect(); }
    vm.heap.get_mut(o).frozen = true;
}

/// A Data instance: an Array underneath, `MRB_TT_STRUCT` (not an Array to the VM).
fn new_instance(vm: &mut Vm, c: ObjId) -> ObjId {
    let o = vm.heap.alloc(c, ObjKind::Array(Default::default()));
    vm.heap.get_mut(o).tt_struct = true;
    o
}

fn alloc(vm: &mut Vm, c: ObjId, vals: &[Value]) -> Value {
    let data = Value::Obj(new_instance(vm, c));
    store(vm, data, vals);
    data
}

fn hash_entries(vm: &Vm, h: Value) -> Vec<(Value, Value)> {
    match h.obj().map(|o| &vm.heap.get(o).kind) { Some(ObjKind::Hash(hd)) => hd.entries().iter().map(|(k, v)| (k.get(), v.get())).collect(), _ => Vec::new() }
}

/// The members as keywords, read as `mrb_get_args`' `:` reads an `mrb_kwargs` (src/class.c):
/// with `required`, the members in order, the first one not given being "missing keyword: x";
/// then the first key that is no member, "unknown keyword: k".
fn take_keywords(vm: &mut Vm, kw: Option<Value>, mems: &[Sym], required: bool) -> VmResult<Vec<Option<Value>>> {
    let entries = kw.map(|k| hash_entries(vm, k)).unwrap_or_default();
    let vals: Vec<Option<Value>> = mems.iter().map(|m| entries.iter().find(|(k, _)| *k == Value::Sym(*m)).map(|e| e.1)).collect();
    if required {
        if let Some(i) = vals.iter().position(|v| v.is_none()) {
            let n = vm.sym_name(mems[i]);
            return Err(vm.raise_arg(&format!("missing keyword: {n}")));
        }
    }
    if let Some((k, _)) = entries.iter().find(|(k, _)| !matches!(k, Value::Sym(s) if mems.contains(s))) {
        return Err(vm.unknown_keyword(*k));
    }
    Ok(vals)
}

fn pending(vm: &Vm, a: &[Value]) -> Option<Value> {
    match (vm.pending_kw, a.last()) { (Some(k), Some(l)) if !k.is_nil() && k == *l => Some(k), _ => None }
}

fn data_initialize(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let c = vm.real_class_of(s);
    let mems = class_members(vm, c)?;
    let kw = pending(vm, a);
    let pos = if kw.is_some() { &a[..a.len() - 1] } else { a };
    // `mrb_data_initialize` reads ":" only: the keywords first ("missing keyword"), then the count
    let vals = take_keywords(vm, kw, &mems, true)?;
    if !pos.is_empty() { return Err(vm.argnum_error(pos.len(), "0")); }
    let vals: Vec<Value> = vals.into_iter().map(|v| v.unwrap_or(Value::Nil)).collect();
    store(vm, s, &vals);
    Ok(s)
}

/// the singleton `new` of a Data class
fn data_new(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let c = s.obj().unwrap();
    let mems = class_members(vm, c)?;
    let n = mems.len();
    let data = Value::Obj(new_instance(vm, c));
    let kw = pending(vm, a);
    let pos = if kw.is_some() { &a[..a.len() - 1] } else { a };
    if !initialize_is(vm, c, data_initialize) {
        // an overridden initialize gets every member as a keyword (positional ones by order)
        if pos.len() > n { return Err(vm.raise_arg(&format!("wrong number of arguments (given {}, expected 0..{})", pos.len(), n))); }
        let h = vm.hash_new();
        for (i, v) in pos.iter().enumerate() { vm.hash_set(h, Value::Sym(mems[i]), *v)?; }
        if let Some(k) = kw { for (kk, kv) in hash_entries(vm, k) { vm.hash_set(h, kk, kv)?; } }
        let fwd = vm.intern("__init_with_kw");
        vm.funcall(data, fwd, &[h], Value::Nil)?;
        return Ok(data);
    }
    let vals: Vec<Value> = if kw.is_some() {
        if !pos.is_empty() { return Err(vm.argnum_error(pos.len(), "0")); }
        take_keywords(vm, kw, &mems, true)?.into_iter().map(|v| v.unwrap_or(Value::Nil)).collect()
    } else {
        if pos.len() != n { return Err(vm.raise_arg("wrong number of arguments")); }
        pos.to_vec()
    };
    store(vm, data, &vals);
    Ok(data)
}

/// `Data.define(*members) { block }`
fn s_def(vm: &mut Vm, s: Value, a: &[Value], b: Value) -> VmResult<Value> {
    let klass = s.obj().unwrap();
    let mut mems: Vec<Sym> = Vec::with_capacity(a.len());
    for v in a { mems.push(sym_arg(vm, *v)?); }
    for i in 0..mems.len() {
        for j in i + 1..mems.len() {
            if mems[i] == mems[j] { let n = vm.sym_name(mems[i]); return Err(vm.raise_arg(&format!("duplicate member: {n}"))); }
        }
    }
    let c = class_new(vm, klass)?;
    let sc = vm.singleton_class(Value::Obj(c))?;
    let define = vm.intern("define");
    let _ = vm.undef_method(sc, define);
    vm.define_methods_args(sc, &[("new", Aspec::any(), data_new), ("members", Aspec::none(), s_members_m)]);
    define_accessors(vm, c, &mems, false);
    let key = vm.intern("__members__");
    let ary = vm.ary_new(mems.iter().map(|m| Value::Sym(*m)).collect());
    vm.heap.ivar_set(c, key, ary);
    // called by a SEND the block runs in a frame of its own
    if !b.is_nil() { return vm.exec_block_with_self_then(b, Value::Obj(c), &[Value::Obj(c)], Value::Obj(c)); }
    Ok(Value::Obj(c))
}

fn init_copy(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    argc!(vm, a, 1);
    if same_object(s, a[0]) { return Ok(s); }
    if vm.real_class_of(a[0]) != vm.real_class_of(s) { return Err(vm.raise_type("wrong argument class")); }
    if !is_struct(vm, a[0]) { return Err(corrupted(vm, "data")); }
    ary_replace(vm, s, a[0])?;
    vm.heap.get_mut(s.obj().unwrap()).frozen = true;
    Ok(s)
}

fn compare(vm: &mut Vm, s: Value, o: Value, eql: bool) -> VmResult<Value> {
    if same_object(s, o) { return Ok(Value::True); }
    if vm.real_class_of(s) != vm.real_class_of(o) { return Ok(Value::False); }
    let (vs, vo) = (values(vm, s), values(vm, o));
    if vs.len() != vo.len() { return Ok(Value::False); }
    let m = vm.intern("eql?");
    for i in 0..vs.len() {
        let eq = if eql { vm.funcall(vs[i], m, &[vo[i]], Value::Nil)?.truthy() } else { vm.equal(vs[i], vo[i])? };
        if !eq { return Ok(Value::False); }
    }
    Ok(Value::True)
}

fn to_s(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    let m = members(vm, s)?;
    let mut out = alloc::string::String::from("#<data ");
    let c = vm.real_class_of(s);
    if vm.heap.class(c).name.is_some() { out.push_str(&vm.class_name(c)); out.push(' '); }
    for (i, mem) in m.iter().enumerate() {
        if i > 0 { out.push_str(", "); }
        out.push_str(&vm.sym_name(*mem));
        out.push('=');
        let v = values(vm, s).get(i).copied().unwrap_or(Value::Nil);
        let t = vm.inspect_str(v)?;
        out.push_str(&t);
    }
    out.push('>');
    Ok(vm.str_from(out))
}

/// `with(**kw)`: a new frozen instance with the given members replaced; `initialize` is not
/// run again
fn data_with(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let c = vm.real_class_of(s);
    let mems = members(vm, s)?;
    let kw = pending(vm, a);
    let pos = if kw.is_some() { &a[..a.len() - 1] } else { a };
    if !pos.is_empty() { return Err(vm.argnum_error(pos.len(), "0")); }
    let Some(kw) = kw else { return Ok(s) };
    let given = take_keywords(vm, Some(kw), &mems, false)?;
    let cur = values(vm, s);
    let vals: Vec<Value> = given.into_iter().enumerate().map(|(i, v)| v.unwrap_or(cur[i])).collect();
    Ok(alloc(vm, c, &vals))
}

pub fn init(vm: &mut Vm) {
    let d = vm.define_class("Data", vm.core.object);
    let sc = vm.singleton_class(Value::Obj(d)).expect("Data singleton");
    let new = vm.intern("new");
    let _ = vm.undef_method(sc, new);
    vm.define_method_args(sc, "define", Aspec::any(), s_def);
    vm.define_methods_args(d, &[
        ("==", Aspec::req(1), |vm, s, a, _b| { compare(vm, s, a[0], false) }),
        ("members", Aspec::none(), m_members),
        ("initialize", Aspec::any(), data_initialize),
        ("initialize_copy", Aspec::any(), init_copy),
        ("eql?", Aspec::req(1), |vm, s, a, _b| { compare(vm, s, a[0], true) }),
        ("with", Aspec::any(), data_with),
        ("to_h", Aspec::none(), |vm, s, _a, _b| { let m = members(vm, s)?; let v = values(vm, s); let h = vm.hash_new(); for (i, mem) in m.iter().enumerate() { vm.hash_set(h, Value::Sym(*mem), v[i])?; } Ok(h) }),
        ("to_s", Aspec::none(), to_s),
        ("inspect", Aspec::none(), to_s),
    ]);
    // `initialize_copy` is `mrb_define_private_method_id` (data.c); `initialize` is written in
    // Ruby (mruby-data mrblib/data.rb), where `mrb_define_method_raw` makes it private
    vm.mark_private(d, &["initialize", "initialize_copy"]);
}
