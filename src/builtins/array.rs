//! Array.

use alloc::{format, vec, vec::Vec};

use crate::error::VmResult;
use crate::object::{Aspec, ArrayData, ObjKind};
use crate::value::{slots_of, values_of, Slot, Value};
use crate::vm::{LoopNext, Vm, LOOP_RESULT};

fn items(vm: &Vm, v: Value) -> Vec<Value> {
    vm.ary_vals(v).unwrap_or_default()
}
/// How many elements, without copying them out (`items(vm, v).len()` allocates).
fn ary_len(vm: &Vm, v: Value) -> usize {
    vm.ary(v).map(|l| l.len()).unwrap_or(0)
}
/// The elements as they lie in the heap, for the natives that only read a few of them.
/// `items` copies the whole array to answer; this borrows it, so the borrow has to end
/// before anything touches `vm` again (nothing here calls Ruby while holding it).
fn slots(vm: &Vm, v: Value) -> &[Slot] {
    vm.ary(v).unwrap_or(&[])
}
fn with_mut<R>(vm: &mut Vm, v: Value, f: impl FnOnce(&mut ArrayData) -> R) -> VmResult<R> {
    match v.obj() {
        Some(o) => {
            if vm.heap.get(o).frozen { return Err(vm.raise(vm.core.frozen_error, "can't modify frozen Array")); }
            match &mut vm.heap.get_mut(o).kind { ObjKind::Array(a) => Ok(f(a)), _ => Err(vm.raise_type("not an array")) }
        }
        None => Err(vm.raise_type("not an array")),
    }
}

pub fn ary_inspect(vm: &mut Vm, v: Value) -> VmResult<Vec<u8>> {
    let list = items(vm, v);
    let mut out = b"[".to_vec();
    for (i, it) in list.iter().enumerate() {
        if i > 0 { out.extend_from_slice(b", "); }
        if *it == v { out.extend_from_slice(b"[...]"); } else { out.extend(vm.inspect(*it)?); }
    }
    out.push(b']');
    Ok(out)
}

fn ary_eq(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    if s == a[0] { return Ok(Value::True); }
    if let (Value::Obj(x), Value::Obj(y)) = (s, a[0]) {
        if vm.eq_guard.contains(&(x, y)) { return Ok(Value::True); }
        vm.eq_guard.push((x, y));
        let r = ary_eq_inner(vm, s, a);
        vm.eq_guard.pop();
        return r;
    }
    ary_eq_inner(vm, s, a)
}

// `ary_eq` (src/array.c): the other side is an Array by `mrb_array_p`, so a Struct or a Data
// is not equal to an Array with the same members
fn ary_eq_inner(vm: &mut Vm, s: Value, a: &[Value]) -> VmResult<Value> {
    let (x, y) = (items(vm, s), match vm.plain_ary_vals(a[0]) { Some(y) => y, None => return Ok(Value::False) });
    if x.len() != y.len() { return Ok(Value::False); }
    for (p, q) in x.iter().zip(y.iter()) { if !vm.equal(*p, *q)? { return Ok(Value::False); } }
    Ok(Value::True)
}

fn ary_cmp(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    // `mrb_ary_cmp`: nil when the other side is not an Array (`mrb_array_p`)
    let (x, y) = (items(vm, s), match vm.plain_ary_vals(a[0]) { Some(y) => y, None => return Ok(Value::Nil) });
    let cmp = vm.intern("<=>");
    for (p, q) in x.iter().zip(y.iter()) {
        let r = vm.funcall(*p, cmp, &[*q], Value::Nil)?;
        match r { Value::Int(0) => {} Value::Int(_) => return Ok(r), _ => return Ok(Value::Nil) }
    }
    Ok(Value::Int(x.len().cmp(&y.len()) as i64))
}

#[cold]
fn sort_failed(vm: &mut Vm) -> crate::error::VmError { vm.raise_arg("comparison failed") }

fn sort_values(vm: &mut Vm, list: &mut Vec<Value>, blk: Value) -> VmResult<()> {
    // Integers only and no block: the comparator below answers `p.cmp(&q)` for every pair and
    // calls nothing, and equal Integers are the same value, so any sort gives the same Array
    if blk.is_nil() && list.iter().all(|v| matches!(v, Value::Int(_))) {
        list.sort_unstable_by_key(|v| match v { Value::Int(i) => *i, _ => 0 });
        return Ok(());
    }
    // insertion-free merge sort via a comparator that may raise: collect errors.
    let cmp = vm.intern("<=>");
    let mut err = None;
    let mut compare = |vm: &mut Vm, a: Value, b: Value| -> core::cmp::Ordering {
        if err.is_some() { return core::cmp::Ordering::Equal; }
        let r = if blk.is_nil() {
            match (a, b) {
                (Value::Int(p), Value::Int(q)) => return p.cmp(&q),
                _ => vm.funcall(a, cmp, &[b], Value::Nil),
            }
        } else { vm.call_block(blk, &[a, b]) };
        match r {
            Ok(Value::Int(i)) => i.cmp(&0),
            // without a block, `mrb_cmp` reports a pair it cannot order (nil, or an answer that
            // is not an Integer) as -2, and `sort_cmp` raises "comparison failed"
            // (src/array.c of mruby 4.1.0-rc2); a block's nil is `cmpint`'s "%T with %T"
            Ok(_) if blk.is_nil() => { err = Some(sort_failed(vm)); core::cmp::Ordering::Equal }
            Ok(Value::Nil) => { let x = vm.describe_for_error(a); let y = vm.describe_for_error(b); err = Some(vm.raise_arg(&format!("comparison of {x} with {y} failed"))); core::cmp::Ordering::Equal }
            Ok(v) => {
                // any other answer is asked `> 0` then `< 0` (a NaN is a tie)
                let (gt, lt) = (vm.s.gt, vm.s.lt);
                match vm.funcall(v, gt, &[Value::Int(0)], Value::Nil) {
                    Ok(t) if t.truthy() => core::cmp::Ordering::Greater,
                    Ok(_) => match vm.funcall(v, lt, &[Value::Int(0)], Value::Nil) { Ok(t) if t.truthy() => core::cmp::Ordering::Less, Ok(_) => core::cmp::Ordering::Equal, Err(e) => { err = Some(e); core::cmp::Ordering::Equal } },
                    Err(e) => { err = Some(e); core::cmp::Ordering::Equal }
                }
            }
            Err(e) => { err = Some(e); core::cmp::Ordering::Equal }
        }
    };
    // simple merge sort (stable) driving the comparator
    let n = list.len();
    let mut buf = list.clone();
    let mut width = 1;
    while width < n {
        let mut i = 0;
        while i < n {
            let mid = (i + width).min(n); let hi = (i + 2 * width).min(n);
            let (mut l, mut r, mut k) = (i, mid, i);
            while l < mid && r < hi {
                if compare(vm, list[r], list[l]) == core::cmp::Ordering::Less { buf[k] = list[r]; r += 1; } else { buf[k] = list[l]; l += 1; }
                k += 1;
            }
            while l < mid { buf[k] = list[l]; l += 1; k += 1; }
            while r < hi { buf[k] = list[r]; r += 1; k += 1; }
            i += 2 * width;
        }
        core::mem::swap(list, &mut buf);
        width *= 2;
    }
    match err { Some(e) => Err(e), None => Ok(()) }
}

// ------------------------------------------------------------------ native loop frames

// The loops of `index { }`, `rindex { }` and `Array.new(n) { }` when a SEND called
// them: each keeps its state in the registers of a native loop frame (`Vm::push_loop_frame`) and
// is stepped once per call of the block, which runs in an ordinary frame above (so a task can
// wait inside it). Each step does what the native's own loop does between two calls of the
// block, in the same order: the same elements, the array read again where the native reads it
// again.

pub(crate) const LOOP_INDEX: i64 = 0;
pub(crate) const LOOP_RINDEX: i64 = 1;
pub(crate) const LOOP_ARY_INIT: i64 = 2;

#[inline]
fn reg(vm: &Vm, base: usize, i: usize) -> Value { vm.stack[base + i].get() }
#[inline]
fn int(vm: &Vm, base: usize, i: usize) -> i64 { match reg(vm, base, i) { Value::Int(n) => n, _ => 0 } }
#[inline]
fn set(vm: &mut Vm, base: usize, i: usize, v: Value) { vm.stack[base + i] = Slot::from(v); }

/// One step of the native loop frame at `base` (`Op::Debug` in `LOOP_IREP`, `Vm::exec_frames`).
pub(crate) fn loop_step(vm: &mut Vm, base: usize) -> VmResult<LoopNext> {
    match int(vm, base, 2) {
        LOOP_INDEX => index_step(vm, base),
        LOOP_RINDEX => rindex_step(vm, base),
        LOOP_ARY_INIT => ary_init_step(vm, base),
        super::ext_catch::LOOP_CATCH => super::ext_catch::catch_step(vm, base),
        k => Err(crate::error::VmError::Internal(format!("no native loop of kind {k}"))),
    }
}

/// `index { }`: R4 the index.
fn index_step(vm: &mut Vm, base: usize) -> VmResult<LoopNext> {
    let s = reg(vm, base, 0);
    let mut i = int(vm, base, 4);
    if int(vm, base, 3) == 1 {
        if reg(vm, base, LOOP_RESULT).truthy() { return Ok(LoopNext::Done(Value::Int(i))); }
        i += 1;
    }
    let it = match vm.ary(s).and_then(|v| v.get(i as usize).map(|x| x.get())) { Some(v) => v, None => return Ok(LoopNext::Done(Value::Nil)) };
    set(vm, base, 3, Value::Int(1));
    set(vm, base, 4, Value::Int(i));
    vm.loop_arg(base, 0, it);
    Ok(LoopNext::Call(1))
}

/// `rindex { }`: R4 the index of the element last given to the block (the length at the start).
fn rindex_step(vm: &mut Vm, base: usize) -> VmResult<LoopNext> {
    let s = reg(vm, base, 0);
    let mut i = int(vm, base, 4) as usize;
    if int(vm, base, 3) == 1 && reg(vm, base, LOOP_RESULT).truthy() { return Ok(LoopNext::Done(Value::Int(i as i64))); }
    while i > 0 {
        i -= 1;
        let len = vm.ary(s).map(|v| v.len()).unwrap_or(0);
        if i >= len { i = len; continue; }
        let it = vm.ary(s).map(|v| v[i].get()).unwrap_or(Value::Nil);
        set(vm, base, 3, Value::Int(1));
        set(vm, base, 4, Value::Int(i as i64));
        vm.loop_arg(base, 0, it);
        return Ok(LoopNext::Call(1));
    }
    Ok(LoopNext::Done(Value::Nil))
}

/// `Array.new(n) { |i| }` (`initialize`): R4 the next index, R5 `n`; each value is written into
/// the receiver as the block gives it (`mrb_ary_set` in rc2's loop).
fn ary_init_step(vm: &mut Vm, base: usize) -> VmResult<LoopNext> {
    let s = reg(vm, base, 0);
    let (i, n) = (int(vm, base, 4), int(vm, base, 5));
    if int(vm, base, 3) == 1 {
        let v = reg(vm, base, LOOP_RESULT);
        with_mut(vm, s, |arr| ary_store(arr, (i - 1) as usize, v))?;
    }
    if i < n {
        set(vm, base, 3, Value::Int(1));
        set(vm, base, 4, Value::Int(i + 1));
        vm.loop_arg(base, 0, Value::Int(i));
        return Ok(LoopNext::Call(1));
    }
    Ok(LoopNext::Done(s))
}

/// `mrb_ary_set` at an index that is not negative: past the end, the gap is filled with nil.
fn ary_store(arr: &mut ArrayData, i: usize, v: Value) {
    if i < arr.len() { arr.as_mut_slice()[i] = Slot::from(v); } else { arr.resize(i, Slot::from(Value::Nil)); arr.push(Slot::from(v)); }
}

pub fn init(vm: &mut Vm) {
    let c = vm.core;
    let sc = vm.singleton_class(Value::Obj(c.array)).unwrap();
    vm.define_method_args(sc, "[]", Aspec::any(), |vm, s, a, _b| { let v = vm.instance_alloc(s.obj().unwrap())?; with_mut(vm, v, |arr| arr.extend(slots_of(a)))?; Ok(v) });
    vm.define_methods_args(c.array, &[
        // `mrb_ary_init`: an Array (`mrb_array_p`) with no second argument (or nil) and no block
        // is a copy; anything else is a size through `mrb_as_int`, then the frozen check, then
        // elements 0...size written over what the array holds (`mrb_ary_set`): rc2 neither clears
        // the rest nor refuses a negative size (`[3, 1, 2].initialize(1, :x)` is `[:x, 1, 2]`,
        // `Array.new(-1)` is `[]`)
        ("initialize", Aspec::opt(2), |vm, s, a, b| {
            let obj = a.get(1).copied().unwrap_or(Value::Nil);
            let size = match a.first() {
                Some(x) if obj.is_nil() && b.is_nil() && vm.plain_ary(*x).is_some() => None,
                None => Some(0),
                Some(Value::Int(n)) => Some(*n),
                Some(x) => Some(super::numeric::as_int(vm, *x)?),
            };
            match size {
                None => { let v = items(vm, a[0]); with_mut(vm, s, |arr| *arr = slots_of(&v).into())?; }
                Some(n) => {
                    with_mut(vm, s, |_| ())?;
                    if !b.is_nil() && vm.in_frame() { return vm.push_loop_frame(LOOP_ARY_INIT, s, b, &[Value::Int(0), Value::Int(n)]); }
                    for i in 0..n.max(0) as usize {
                        let v = if b.is_nil() { obj } else { vm.call_block(b, &[Value::Int(i as i64)])? };
                        with_mut(vm, s, |arr| ary_store(arr, i, v))?;
                    }
                }
            }
            Ok(s)
        }),
        // `mrb_ary_replace_m` for both: `mrb_get_args("A")`
        // the slots are copied once: `Kernel#dup` leaves the copy empty for this to fill
        ("initialize_copy", Aspec::req(1), |vm, s, a, _b| { let v: Vec<Slot> = match vm.plain_ary(a[0]) { Some(x) => x.to_vec(), None => slots_of(&vm.ensure_array(a[0])?) }; with_mut(vm, s, |arr| *arr = v.into())?; Ok(s) }),
        ("replace", Aspec::req(1), |vm, s, a, _b| { let v = vm.ensure_array(a[0])?; with_mut(vm, s, |arr| *arr = slots_of(&v).into())?; Ok(s) }),
        ("size", Aspec::none(), |vm, s, _a, _b| Ok(Value::Int(vm.ary(s).map(|v| v.len()).unwrap_or(0) as i64))),
        ("length", Aspec::none(), |vm, s, _a, _b| Ok(Value::Int(vm.ary(s).map(|v| v.len()).unwrap_or(0) as i64))),
        ("empty?", Aspec::none(), |vm, s, _a, _b| Ok(Value::bool(vm.ary(s).map(|v| v.is_empty()).unwrap_or(true)))),
        ("[]", Aspec::req(1) | Aspec::opt(1), ary_aref),
        ("slice", Aspec::req(1) | Aspec::opt(1), ary_aref),
        ("[]=", Aspec::req(2) | Aspec::opt(1), ary_aset),
        ("first", Aspec::opt(1), |vm, s, a, _b| { if a.is_empty() { return Ok(vm.ary(s).and_then(|v| v.first().map(|x| x.get())).unwrap_or(Value::Nil)); } let n = vm.expect_int(a[0], "argument")?; if n < 0 { return Err(vm.raise_arg("negative array size")); } let v: Vec<Value> = slots(vm, s).iter().take(n as usize).map(|x| x.get()).collect(); Ok(vm.ary_new(v)) }),
        ("last", Aspec::opt(1), |vm, s, a, _b| { if a.is_empty() { return Ok(vm.ary(s).and_then(|v| v.last().map(|x| x.get())).unwrap_or(Value::Nil)); } let n = vm.expect_int(a[0], "argument")?; if n < 0 { return Err(vm.raise_arg("negative array size")); } let l = slots(vm, s); let n = (n as usize).min(l.len()); let v: Vec<Value> = l[l.len() - n..].iter().map(|x| x.get()).collect(); Ok(vm.ary_new(v)) }),
        ("<<", Aspec::req(1), |vm, s, a, _b| { with_mut(vm, s, |arr| arr.push(Slot::from(a[0])))?; Ok(s) }),
        ("push", Aspec::any(), |vm, s, a, _b| { with_mut(vm, s, |arr| arr.extend(slots_of(a)))?; Ok(s) }),
        ("append", Aspec::any(), |vm, s, a, _b| { with_mut(vm, s, |arr| arr.extend(slots_of(a)))?; Ok(s) }),
        // `mrb_ary_pop` takes no argument (`MRB_ARGS_NONE`, src/array.c of mruby 4.1.0-rc2)
        ("pop", Aspec::none(), |vm, s, _a, _b| { with_mut(vm, s, |arr| arr.pop().map(|x| x.get()).unwrap_or(Value::Nil)) }),
        ("shift", Aspec::opt(1), |vm, s, a, _b| { if a.is_empty() { return with_mut(vm, s, |arr| arr.shift().map(|x| x.get()).unwrap_or(Value::Nil)); } let n = vm.expect_int(a[0], "argument")? as usize; let v = with_mut(vm, s, |arr| values_of(&arr.shift_n(n)))?; Ok(vm.ary_new(v)) }),
        ("unshift", Aspec::any(), |vm, s, a, _b| { with_mut(vm, s, |arr| arr.unshift(slots_of(a)))?; Ok(s) }),
        ("prepend", Aspec::any(), |vm, s, a, _b| { with_mut(vm, s, |arr| arr.unshift(slots_of(a)))?; Ok(s) }),
        // `mrb_ary_concat_m` (`MRB_ARGS_REQ(1)` in the method table) and `mrb_ary_plus`
        // (`mrb_get_args("a")`): the argument goes through `mrb_ensure_array_type`
        ("concat", Aspec::req(1), |vm, s, a, _b| { let all = vm.ensure_array(a[0])?; with_mut(vm, s, |arr| arr.extend(slots_of(&all)))?; Ok(s) }),
        ("+", Aspec::req(1), |vm, s, a, _b| { let mut v = items(vm, s); let o = vm.ensure_array(a[0])?; v.extend(o); Ok(vm.ary_new(v)) }),
        // `mrb_ary_times`: a String joins, anything else is a count through `mrb_as_int`
        ("*", Aspec::req(1), |vm, s, a, _b| { if let Some(sep) = vm.str_bytes(a[0]).map(|b| b.to_vec()) { return ary_join(vm, s, &sep); } let n = vm.expect_int(a[0], "argument")?; if n < 0 { return Err(vm.raise_arg("negative argument")); } let v = items(vm, s).repeat(n as usize); Ok(vm.ary_new(v)) }),
        ("==", Aspec::req(1), ary_eq),
        ("eql?", Aspec::req(1), |vm, s, a, _b| { if s == a[0] { return Ok(Value::True); } let (x, y) = (items(vm, s), match vm.plain_ary_vals(a[0]) { Some(y) => y, None => return Ok(Value::False) }); if x.len() != y.len() { return Ok(Value::False); } let pair = (s.obj().unwrap(), a[0].obj().unwrap()); if vm.eq_guard.contains(&pair) { return Ok(Value::True); } vm.eq_guard.push(pair); let eql = vm.s.eql; let mut res = Ok(Value::True); for (p, q) in x.iter().zip(y.iter()) { match vm.funcall(*p, eql, &[*q], Value::Nil) { Ok(t) if t.truthy() => {} Ok(_) => { res = Ok(Value::False); break; } Err(e) => { res = Err(e); break; } } } vm.eq_guard.pop(); res }),
        ("<=>", Aspec::req(1), ary_cmp),
        ("__ary_eq", Aspec::req(1), ary_eq),
        ("__ary_cmp", Aspec::req(1), ary_cmp),
        ("__ary_index", Aspec::req(1), |vm, s, a, _b| { let mut i = 0; loop { let it = match slots(vm, s).get(i).map(|x| x.get()) { Some(v) => v, None => break }; if vm.equal(it, a[0])? { return Ok(Value::Int(i as i64)); } i += 1; } Ok(Value::Nil) }),
        ("__svalue", Aspec::none(), |vm, s, _a, _b| { let l = slots(vm, s); Ok(if l.len() == 1 { l[0].get() } else if l.is_empty() { Value::Nil } else { s }) }),
        ("inspect", Aspec::none(), |vm, s, _a, _b| { let b = ary_inspect(vm, s)?; Ok(vm.str_new(&b)) }),
        ("to_s", Aspec::none(), |vm, s, _a, _b| { let b = ary_inspect(vm, s)?; Ok(vm.str_new(&b)) }),
        ("to_a", Aspec::none(), |_vm, s, _a, _b| Ok(s)),
        ("entries", Aspec::none(), |_vm, s, _a, _b| Ok(s)),
        ("to_ary", Aspec::any(), |_vm, s, _a, _b| Ok(s)),
        // to_h / zip are mruby-array-ext (gem) methods, provided natively here.
        ("join", Aspec::opt(1), |vm, s, a, _b| { let sep = match a.first() { Some(v) if !v.is_nil() => vm.expect_str(*v, "separator")?, _ => vec![] }; ary_join(vm, s, &sep) }),
        ("reverse", Aspec::none(), |vm, s, _a, _b| { let mut v = items(vm, s); v.reverse(); Ok(vm.ary_new(v)) }),
        ("reverse!", Aspec::none(), |vm, s, _a, _b| { with_mut(vm, s, |arr| arr.reverse())?; Ok(s) }),
        // with a block and called by a SEND, the loop is a native loop frame (`loop_step`), so
        // the block runs in an ordinary frame (`docs/design/wait-anywhere.md`)
        // `mrb_ary_index_m`: no argument and no block is `to_enum(:index)`
        ("index", Aspec::opt(1), |vm, s, a, b| { if a.is_empty() && b.is_nil() { let (te, n) = (vm.intern("to_enum"), vm.intern("index")); return vm.funcall(s, te, &[Value::Sym(n)], Value::Nil); } if a.is_empty() && !b.is_nil() && vm.in_frame() { return vm.push_loop_frame(LOOP_INDEX, s, b, &[Value::Int(0)]); } let mut i = 0; loop { let it = match vm.ary(s).and_then(|v| v.get(i).map(|x| x.get())) { Some(v) => v, None => return Ok(Value::Nil) }; let hit = if let Some(x) = a.first() { vm.equal(it, *x)? } else { vm.call_block(b, &[it])?.truthy() }; if hit { return Ok(Value::Int(i as i64)); } i += 1; } }),
        ("rindex", Aspec::opt(1), |vm, s, a, b| {
            // `mrb_ary_rindex_m`: no argument and no block is `to_enum(:rindex)`
            if a.is_empty() && b.is_nil() { let (te, n) = (vm.intern("to_enum"), vm.intern("rindex")); return vm.funcall(s, te, &[Value::Sym(n)], Value::Nil); }
            if a.is_empty() && !b.is_nil() && vm.in_frame() { let n = ary_len(vm, s) as i64; return vm.push_loop_frame(LOOP_RINDEX, s, b, &[Value::Int(n)]); }
            // the array is re-read every step: `==` or the block may shrink or replace it
            let mut i = vm.ary(s).map(|v| v.len()).unwrap_or(0);
            while i > 0 {
                i -= 1;
                let len = vm.ary(s).map(|v| v.len()).unwrap_or(0);
                if i >= len { i = len; continue; }
                let it = vm.ary(s).map(|v| v[i].get()).unwrap_or(Value::Nil);
                let hit = if let Some(x) = a.first() { vm.equal(it, *x)? } else { vm.call_block(b, &[it])?.truthy() };
                if hit { return Ok(Value::Int(i as i64)); }
            }
            Ok(Value::Nil)
        }),
        ("clear", Aspec::none(), |vm, s, _a, _b| { with_mut(vm, s, |arr| arr.clear())?; Ok(s) }),
        ("delete_at", Aspec::req(1), |vm, s, a, _b| { let i = match a[0] { Value::Int(i) => i, v => super::numeric::as_int(vm, v)? }; with_mut(vm, s, |arr| { let i = if i < 0 { i + arr.len() as i64 } else { i }; if i < 0 || i as usize >= arr.len() { Value::Nil } else { arr.remove(i as usize).get() } }) }),
        // `mrb_ary_delete` checks frozen only when it moves an element: nothing found is no write
        // (a frozen array answers nil or the block). rc2 also shortens a frozen array whose only
        // match is the last element; that write stays a FrozenError here (worklog Y2)
        ("delete", Aspec::req(1), |vm, s, a, b| { let list = items(vm, s); let mut keep = vec![]; let mut found = None; for it in list { if vm.equal(it, a[0])? { found = Some(it); } else { keep.push(it); } } if found.is_some() { with_mut(vm, s, |arr| *arr = slots_of(&keep).into())?; } match found { Some(v) => Ok(v), None => if b.is_nil() { Ok(Value::Nil) } else { vm.exec_block(b, &[a[0]]) } } }),
        // `sort! { }` keeps its nested loop: a loop frame cost a light block 8% (`sort { |x, y| x <=> y }`,
        // docs/verification/bench.md), and the author chose the speed (2026-09-26); it stays a named
        // boundary (`docs/design/wait-anywhere.md`)
        ("sort!", Aspec::none(), |vm, s, _a, b| { let mut v = items(vm, s); sort_values(vm, &mut v, b)?; with_mut(vm, s, |arr| *arr = slots_of(&v).into())?; Ok(s) }),
    ]);
    // `MRB_MT_PRIVATE` in the reference's ROM table for this class (src/array.c)
    vm.mark_private(c.array, &["initialize", "initialize_copy"]);
}

fn ary_join(vm: &mut Vm, s: Value, sep: &[u8]) -> VmResult<Value> {
    // `join_ary`: the separator goes before every element but the first of each array, nested
    // ones included, so an empty nested array still has its separators (`[[], 1].join(", ")` is
    // `", 1"`); a nested array that is the array being joined or one of the arrays it is inside
    // is "recursive array join"
    fn rec(vm: &mut Vm, v: Value, sep: &[u8], out: &mut Vec<u8>, path: &mut Vec<Value>) -> VmResult<()> {
        for (idx, it) in items(vm, v).into_iter().enumerate() {
            if idx > 0 { out.extend_from_slice(sep); }
            // an element is joined as an Array when `mrb_array_p` (or `mrb_check_array_type`,
            // the same test) says so; a Struct goes through `to_s`
            if vm.plain_ary(it).is_some() {
                if it == v || path.contains(&it) { return Err(vm.raise_arg("recursive array join")); }
                path.push(v);
                rec(vm, it, sep, out, path)?;
                path.pop();
                continue;
            }
            out.extend(vm.as_string(it)?);
        }
        Ok(())
    }
    let mut out = vec![];
    rec(vm, s, sep, &mut out, &mut Vec::new())?;
    Ok(vm.str_new(&out))
}

/// `aget_index` (src/array.c of mruby 4.1.0-rc2): an Integer as it is, anything else through
/// `mrb_as_int`. The reference narrows a Float with the C cast `(mrb_int)f`, which is undefined
/// for a NaN, an infinity and a value past `mrb_int` (x86-64 answers the smallest Integer,
/// AArch64 0 for a NaN); mruby master e2148c1b1 fixed it as a bug by taking `mrb_as_int`
/// there too, and that is the answer here (RangeError; README, "Known deviations"). The
/// reference reaches the other values by `mrb_get_args(mrb, "i*!")`, which converts the
/// method's first argument; every caller of `Array#[]` passes the index there, so that is the
/// value converted here too.
fn aget_index(vm: &mut Vm, v: Value) -> VmResult<i64> {
    match v {
        Value::Int(i) => Ok(i),
        v => super::numeric::as_int(vm, v),
    }
}

/// `Array#[]` (`mrb_ary_aget`, src/array.c of mruby 4.1.0-rc2). Its own reading of the
/// arguments, not `String#[]`'s (`string::index_args`): a Float index is a C cast here and
/// `mrb_as_int` there, and with two arguments the length is converted before the index.
fn ary_aref(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    if let [v] = *a {
        let i = match v {
            // hot path (`each` in mrblib indexes with `self[idx]`): no copy of the array
            Value::Int(i) => i,
            // a[n..m]: `mrb_range_beg_len(..., TRUE)`, whose ends go through `mrb_as_int`
            Value::Obj(_) if super::range::is_range_obj(vm, v) => {
                let len = ary_len(vm, s) as i64;
                // the conversions are done before the borrow (an end may run `to_i` of a
                // Rational), and the range is clipped to the array as it is now
                return match super::ext_array::range_beg_len(vm, v, len, true)? {
                    Some((i, n)) => {
                        let list = slots(vm, s);
                        let (i, n) = ((i as usize).min(list.len()), n as usize);
                        let v: Vec<Value> = list[i..(i + n).min(list.len())].iter().map(|x| x.get()).collect();
                        Ok(vm.ary_new(v))
                    }
                    None => Ok(Value::Nil),
                };
            }
            v => aget_index(vm, v)?,
        };
        // `mrb_ary_ref`
        let list = match vm.ary(s) { Some(l) => l, None => return Ok(Value::Nil) };
        let i = if i < 0 { i + list.len() as i64 } else { i };
        return Ok(if i < 0 { Value::Nil } else { list.get(i as usize).map(|x| x.get()).unwrap_or(Value::Nil) });
    }
    // `mrb_get_args(mrb, "oi", &index, &len)` converts the length first, then `aget_index`
    let n = match a[1] { Value::Int(n) => n, v => super::numeric::as_int(vm, v)? };
    let i = aget_index(vm, a[0])?;
    let alen = ary_len(vm, s) as i64;
    let i = if i < 0 { i + alen } else { i };
    if i < 0 || alen < i || n < 0 { return Ok(Value::Nil); }
    let n = n.min(alen - i);
    let v: Vec<Value> = slots(vm, s)[i as usize..(i + n) as usize].iter().map(|x| x.get()).collect();
    Ok(vm.ary_new(v))
}

/// `Array#[]=`. `pub(crate)` because `OP_SETIDX` records it as the implementation it stands
/// in for and calls it directly for an Integer index (`Vm::op_setidx`).
pub(crate) fn ary_aset(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    // a Float index or length goes through `mrb_as_int` as everything else does: the
    // reference's C cast is undefined for a NaN, an infinity or a value past `mrb_int`, and
    // mruby master e2148c1b1 made it `mrb_as_int` (`aget_index` above)
    let val = a[a.len() - 1];
    let len = ary_len(vm, s);
    // `mrb_ary_aset` (src/array.c of mruby 4.1.0-rc2): a Range goes through
    // `mrb_range_beg_len(..., FALSE)`, which takes its ends with `mrb_as_int` (a Float end is
    // truncated) and where a start before the array is a RangeError and one past the end pads
    if let [Value::Obj(_), _] = a {
        if super::range::is_range_obj(vm, a[0]) {
            let (begin, end, excl) = super::range::parts(vm, a[0])?;
            let b = match begin { Value::Int(b) => b, Value::Nil => 0, v => super::numeric::as_int(vm, v)? };
            let (e, excl) = match end { Value::Int(e) => (e, excl), Value::Nil => (-1, false), v => (super::numeric::as_int(vm, v)?, excl) };
            let b = if b < 0 { b + len as i64 } else { b };
            if b < 0 {
                let d = vm.as_string(a[0])?;
                return Err(vm.raise(vm.core.range_error, &format!("{} out of range", alloc::string::String::from_utf8_lossy(&d))));
            }
            let e = if e < 0 { e + len as i64 } else { e } + if excl { 0 } else { 1 };
            let (i, n) = (b as usize, (e - b).max(0) as usize);
            let rep = match vm.plain_ary_vals(val) { Some(v) => v, None => vec![val] };
            with_mut(vm, s, |arr| { if arr.len() < i { arr.resize(i, Slot::NIL); } let end = (i + n).min(arr.len()); arr.splice_range(i, end, slots_of(&rep)); })?;
            return Ok(val);
        }
    }
    // `aget_index`: an index or length that is neither an Integer nor a Float goes through
    // `mrb_as_int` (`nil cannot be converted to Integer`). The reference converts the method's
    // first argument there (`mrb_get_args(mrb, "i*!", ...)`), so for a length that is not a
    // number it reads the index instead; this converts the length itself.
    let h = match a[0] { Value::Int(i) => i, v => super::numeric::as_int(vm, v)? };
    if a.len() == 2 {
        let i = if h < 0 { h + len as i64 } else { h };
        // `mrb_ary_set` (src/array.c): the frozen check (`ary_modify`) first, then the index as
        // it was given
        with_mut(vm, s, |_| ())?;
        if i < 0 { return Err(vm.raise(vm.core.index_error, &format!("index {} out of array", i - len as i64))); }
        with_mut(vm, s, |arr| { let i = i as usize; if arr.len() <= i { arr.resize(i + 1, Slot::NIL); } arr[i] = Slot::from(val); })?;
        return Ok(val);
    }
    // `a[i, n] = v` is `mrb_ary_splice`, whose message gives the index after the length of the
    // array was added to it
    let n = match a[1] { Value::Int(n) => n, v => super::numeric::as_int(vm, v)? };
    // `mrb_ary_splice` opens with `ary_modify`
    with_mut(vm, s, |_| ())?;
    if n < 0 { return Err(vm.raise(vm.core.index_error, &format!("negative length ({n})"))); }
    if h < 0 && h + (len as i64) < 0 { return Err(vm.raise(vm.core.index_error, &format!("index {} is out of array", h + len as i64))); }
    let (i, n) = match super::string::index_args(vm, len, &[Value::Int(h), Value::Int(n)])? {
        Some(x) => x,
        // start beyond the end: pad with nil
        None => (h as usize, 0),
    };
    // `mrb_ary_splice`: the replacement is spread when `mrb_array_p`
    let rep = match vm.plain_ary_vals(val) { Some(v) => v, None => vec![val] };
    with_mut(vm, s, |arr| { if arr.len() < i { arr.resize(i, Slot::NIL); } let end = (i + n).min(arr.len()); arr.splice_range(i, end, slots_of(&rep)); })?;
    Ok(val)
}
