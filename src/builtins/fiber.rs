//! `Fiber` (mruby-fiber gem, `mrbgems/mruby-fiber/src/fiber.c`). The context
//! switching itself lives in `Vm` (`fiber_switch`, `fiber_yield`, ...); these
//! are the Ruby-visible methods.

use alloc::{format, vec::Vec};

use crate::error::VmResult;
use crate::object::{Aspec, ObjKind};
use crate::value::Value;
use crate::vm::{FiberState, Vm};

pub fn init(vm: &mut Vm) {
    let fiber = vm.core.fiber;
    vm.define_methods_args(fiber, &[
        ("initialize", Aspec::block(), |vm, s, a, b| {
            if !a.is_empty() { return Err(vm.argnum_error(a.len(), "0")); }
            let p = match b { Value::Obj(o) if matches!(vm.heap.get(o).kind, ObjKind::Proc(_)) => o, _ => return Err(vm.raise_arg("no block given")) };
            vm.fiber_init(s, p)?;
            Ok(s)
        }),
        ("resume", Aspec::any(), |vm, s, a, _b| {
            // called from bytecode: switch inside the running loop; from native code: nested loop
            let vmexec = !vm.direct_send;
            vm.fiber_switch(s, a, true, vmexec)
        }),
        ("transfer", Aspec::any(), |vm, s, a, _b| vm.fiber_transfer(s, a)),
        ("alive?", Aspec::none(), |vm, s, _a, _b| Ok(Value::bool(vm.fiber_state(s)? != FiberState::Terminated))),
        ("==", Aspec::req(1), |_vm, s, a, _b| { if a.len() != 1 { return Ok(Value::False); } Ok(Value::bool(matches!((s, a[0]), (Value::Obj(x), Value::Obj(y)) if x == y))) }),
        ("to_s", Aspec::none(), fiber_to_s),
        ("inspect", Aspec::none(), fiber_to_s),
    ]);
    // `mrb_define_method_id(..., MRB_SYM(initialize), fiber_init, ...)` (mruby-fiber
    // fiber.c): no flag of its own, but `mrb_define_method_raw` makes every `initialize`
    // private, and that path is the one a C definition outside a ROM table takes
    vm.mark_private(fiber, &["initialize"]);
    let sc = vm.singleton_class(Value::Obj(fiber)).expect("Fiber singleton");
    vm.define_methods_args(sc, &[
        ("yield", Aspec::any(), |vm, _s, a, _b| vm.fiber_yield(a)),
        ("current", Aspec::none(), |vm, _s, _a, _b| Ok(vm.fiber_current())),
    ]);
}

/// `#<Fiber:0x... file:line (status)>` (`fiber_to_s`, mruby-fiber): the position is where the
/// context's first frame begins (`mrb_debug_get_position` at pc 0 of `cibase->proc`) — the
/// fiber's block, or the top-level program for the root fiber — and is left out once the fiber
/// has terminated or where there is no debug information.
fn fiber_to_s(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    let state = vm.fiber_state(s)?;
    let pos = if state == FiberState::Terminated { None } else { fiber_position(vm, s) };
    let st = match state {
        FiberState::Created => "created",
        FiberState::Running => "resumed",
        FiberState::Resumed => "suspended by resuming",
        FiberState::Suspended | FiberState::Transferred => "suspended",
        FiberState::Terminated => "terminated",
    };
    let base = super::object::any_to_s(vm, s);
    let body: Vec<u8> = base.as_bytes()[..base.len() - 1].to_vec();
    let pos = pos.map(|p| format!(" {p}")).unwrap_or_default();
    let text = format!("{}{pos} ({st})>", alloc::string::String::from_utf8_lossy(&body));
    Ok(vm.str_from(text))
}

fn fiber_position(vm: &mut Vm, s: Value) -> Option<alloc::string::String> {
    let c = match s.obj().map(|o| &vm.heap.get(o).kind) { Some(ObjKind::Fiber(c)) if *c != usize::MAX => *c, _ => return None };
    let irep = match vm.contexts[c].proc_ {
        Some(p) => vm.heap.proc_data(p).irep,
        None => { let ci = if c == vm.cur { &vm.ci } else { &vm.contexts[c].ci }; ci.first()?.irep }
    };
    let ir = &vm.ireps[irep];
    let (line, file) = (ir.line_of(0)?, ir.filename.clone()?);
    Some(format!("{file}:{line}"))
}
