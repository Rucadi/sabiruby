//! BasicObject / Object / Module / Class / NilClass / TrueClass / FalseClass.

use alloc::{format, string::String, vec, vec::Vec};

use crate::argc;
use crate::error::VmResult;
use crate::object::{Aspec, Method, ObjKind, Vis};
use crate::value::{ObjId, Slot, Value};
use crate::vm::Vm;

/// `Class#allocate`, the one `Class#new` skips the dispatch for.
fn default_allocate(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    match s.obj() { Some(c) => vm.instance_alloc(c), None => Err(vm.raise_type("not a class")) }
}

/// The proc `define_method` and `define_singleton_method` store: a copy of the block with
/// `MRB_PROC_STRICT` set (`define_method_m`, src/class.c: `mrb_proc_copy`, then
/// `p->flags |= MRB_PROC_STRICT`), so the method checks its arguments as a lambda does and its
/// `arity` is a lambda's; the block itself is left as it was. A copy without an environment
/// takes the class it is defined in (`mrb_define_method_raw`).
fn method_proc(vm: &mut Vm, p: ObjId, class: ObjId) -> ObjId {
    let mut pd = vm.heap.proc_data(p).clone();
    pd.strict = true;
    if pd.env.is_none() { pd.target_class = Some(class); }
    let cls = vm.heap.get(p).class;
    vm.heap.alloc(cls, ObjKind::Proc(pd))
}

pub fn init(vm: &mut Vm) {
    let c = vm.core;
    // `bob_rom_entries` of src/class.c marks these `MRB_MT_PRIVATE`. `initialize` is there
    // too but needs no marking: `Vm::def_method_raw` makes it private wherever it is defined.
    vm.define_private_methods_args(c.basic_object, &[
        // the three hooks a change to an object's singleton class fires, as no-ops
        // (`mrb_do_nothing` for all three). The definition being the one here is what lets
        // `Vm::method_table_hook` skip the call entirely.
        ("singleton_method_added", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("singleton_method_removed", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("singleton_method_undefined", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("method_missing", Aspec::any(), method_missing),
    ]);
    vm.define_methods_args(c.basic_object, &[
        // `MRB_MT_PRIVATE` in `bob_rom_entries`; marked below, where the table is complete
        ("initialize", Aspec::none(), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("==", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(a.first().map(|x| vm.same_value(*x, s)).unwrap_or(false))) }),
        ("equal?", Aspec::req(1), |_vm, s, a, _b| Ok(Value::bool(a.first().map(|x| same_object(*x, s)).unwrap_or(false)))),
        ("!", Aspec::none(), |_vm, s, _a, _b| Ok(Value::bool(!s.truthy()))),
        ("!=", Aspec::req(1), |vm, s, a, _b| { let r = vm.equal(s, a[0])?; Ok(Value::bool(!r)) }),
        ("__id__", Aspec::none(), |_vm, s, _a, _b| { Ok(object_id(s)) }),
        ("__send__", Aspec::req(1) | Aspec::rest() | Aspec::block(), send),
        ("instance_eval", Aspec::opt(3) | Aspec::block(), |vm, s, _a, b| { if b.is_nil() { return Err(vm.raise_arg("no block given")); } vm.exec_block_with_self(b, s, &[s], None) }),
    ]);
    vm.mark_private(c.basic_object, &["initialize"]);
    vm.define_methods_args(c.object, &[
        ("class", Aspec::none(), |vm, s, _a, _b| Ok(Value::Obj(vm.real_class_of(s)))),
        ("singleton_class", Aspec::none(), |vm, s, _a, _b| Ok(Value::Obj(vm.singleton_class(s)?))),
        ("hash", Aspec::none(), |vm, s, _a, _b| Ok(Value::Int(vm.value_hash(s)))),
        ("eql?", Aspec::req(1), |vm, s, a, _b| Ok(Value::bool(a.first().map(|x| vm.same_value(*x, s)).unwrap_or(false)))),
        ("===", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(vm.equal(s, a[0])?)) }),

        ("nil?", Aspec::none(), |_vm, s, _a, _b| Ok(Value::bool(s.is_nil()))),
        ("to_s", Aspec::none(), |vm, s, _a, _b| { let t = any_to_s(vm, s); Ok(vm.str_from(t)) }),
        ("inspect", Aspec::none(), obj_inspect),
        ("is_a?", Aspec::req(1), is_a),
        ("kind_of?", Aspec::req(1), is_a),
        ("instance_of?", Aspec::req(1), |vm, s, a, _b| { let c = class_arg(vm, a[0])?; Ok(Value::bool(vm.real_class_of(s) == c)) }),
        // `obj_respond_to` (src/kernel.c). The second argument reaches `respond_to_missing?`
        // and nothing else: for a method that *is* there mruby answers true whatever its
        // visibility, where CRuby would answer false for a private one without `include_all`.
        // `Object.new.respond_to?(:puts)` is true in the reference. The one false a found
        // method gets is a body that is unimplemented on this machine (`MRB_METHOD_NOTIMPL_P`).
        ("respond_to?", Aspec::req(1) | Aspec::opt(1), |vm, s, a, _b| { let m = sym_arg(vm, a[0])?; if let Some((mth, _owner)) = vm.find_method(vm.class_of(s), m) { if let Method::Native(f, _) = mth { if vm.notimpl_fns.iter().any(|g| core::ptr::fn_addr_eq(*g, f)) { return Ok(Value::False); } } return Ok(Value::True); } let rtm = vm.intern("respond_to_missing?"); let priv_ = a.get(1).copied().unwrap_or(Value::False); let r = vm.funcall(s, rtm, &[Value::Sym(m), priv_], Value::Nil)?; Ok(Value::bool(r.truthy())) }),
        ("respond_to_missing?", Aspec::req(1) | Aspec::opt(1), |_vm, _s, _a, _b| Ok(Value::False)),
        // `mrb_obj_remove_instance_variable` (src/kernel.c of 4.1.0-rc2): the name is checked
        // (`mrb_iv_name_sym_check`) before the receiver
        ("remove_instance_variable", Aspec::req(1), |vm, s, a, _b| { let n = super::ext_metaprog::iv_check(vm, a[0])?; match vm.iv_remove(s, n)? { Some(v) => Ok(v), None => { let nn = vm.sym_name(n); Err(vm.name_error(n, &format!("instance variable {nn} not defined"))) } } }),
        ("send", Aspec::req(1) | Aspec::rest() | Aspec::block(), send),
        ("dup", Aspec::none(), dup),
        ("clone", Aspec::none(), clone),
        ("freeze", Aspec::none(), |vm, s, _a, _b| { if let Value::Obj(o) = s { vm.heap.get_mut(o).frozen = true; let c = vm.heap.get(o).class; if vm.heap.class(c).is_singleton { vm.heap.get_mut(c).frozen = true; } } Ok(s) }),
        ("frozen?", Aspec::none(), |vm, s, _a, _b| Ok(Value::bool(match s { Value::Obj(o) => vm.heap.get(o).frozen, _ => true }))),
        // Known deviation: rc2 declares `MRB_ARGS_REQ(1)|MRB_ARGS_BLOCK()` (mruby-metaprog
        // metaprog.c), which refuses the Proc its own body reads ("n|o&"); declared as
        // `define_method` is, as CRuby takes it (README, tests/custom/define_singleton_method_proc.rb)
        ("define_singleton_method", Aspec::req(1) | Aspec::opt(1), |vm, s, a, b| { let n = sym_arg(vm, a[0])?; let body = if a.len() == 2 { a[1] } else { b }; let sc = vm.singleton_class(s)?; match body { Value::Obj(p) if matches!(vm.heap.get(p).kind, ObjKind::Proc(_)) => { let p = method_proc(vm, p, sc); vm.def_method(sc, n, Method::Ruby(p), Vis::Public)?; Ok(Value::Sym(n)) } Value::Nil if a.len() == 1 => Err(vm.raise_arg("no block given")), v => { let d = vm.describe_for_error(v); Err(vm.raise_type(&format!("wrong argument type {d} (expected Proc)"))) } } }),
        // `mrb_include_module` checks the singleton class, so the message names Class
        ("extend", Aspec::any(), |vm, s, a, _b| { if let Value::Obj(o) = s { if vm.heap.get(o).frozen { let sc = vm.singleton_class(s)?; return Err(vm.frozen_error(Value::Obj(sc))); } } for m in a.iter().rev() { let m = module_arg(vm, *m)?; let sc = vm.singleton_class(s)?; /* `mrb_include_module` checks the singleton class itself, frozen on its own or not */ if vm.heap.get(sc).frozen { return Err(vm.frozen_error(Value::Obj(sc))); } vm.include_module(sc, m); let hook = vm.intern("extended"); vm.funcall(Value::Obj(m), hook, &[s], Value::Nil)?; } Ok(s) }),
    ]);
    // `MRB_MT_PRIVATE` in `krn_rom_entries` (src/kernel.c): these natives sit on Object
    // here and `ext_metaprog.rs` moves them to Kernel with their visibility
    vm.mark_private(c.object, &["respond_to_missing?"]);

    // Module
    vm.define_methods_args(c.module, &[
        // name, <, <=, <=>, >, >= are mruby-class-ext (`ext_class.rs`)
        ("to_s", Aspec::none(), mod_to_s),
        ("inspect", Aspec::none(), mod_to_s),
        ("===", Aspec::req(1), |vm, s, a, _b| { Ok(Value::bool(vm.obj_is_kind_of(a[0], s.obj().unwrap()))) }),
        // `mrb_mod_dup` (src/class.c): `mrb_obj_clone` with the frozen flag cleared. Module has
        // no `==` or `initialize_copy` of its own (BasicObject's and Kernel's)
        ("dup", Aspec::none(), |vm, s, _a, _b| { let d = clone(vm, s, &[], Value::Nil)?; if let Value::Obj(o) = d { vm.heap.get_mut(o).frozen = false; } Ok(d) }),
        ("include", Aspec::any(), |vm, s, a, _b| { let cls = s.obj().unwrap(); if vm.heap.get(cls).frozen { return Err(vm.frozen_error(s)); } for mv in a.iter().rev() { let m = module_arg(vm, *mv)?; vm.include_module(cls, m); let hook = vm.intern("included"); vm.funcall(Value::Obj(m), hook, &[s], Value::Nil)?; } Ok(s) }),
        // `mrb_const_missing` (src/class.c): `mrb_class_real` of the receiver decides whether the
        // name is qualified, so `Object` and nothing else reads unqualified
        ("const_missing", Aspec::req(1), |vm, s, a, _b| { let n = sym_arg(vm, a[0])?; let nn = vm.sym_name(n); let o = s.obj().unwrap(); let msg = if vm.class_real(o) == Some(vm.core.object) { format!("uninitialized constant {nn}") } else { let cn = module_str(vm, o); format!("uninitialized constant {cn}::{nn}") }; Err(vm.name_error(n, &msg)) }),
        ("include?", Aspec::req(1), |vm, s, a, _b| { class_arg(vm, a[0])?; let m = module_arg(vm, a[0])?; let mut c = vm.heap.class(s.obj().unwrap()).superclass; while let Some(x) = c { if vm.heap.class(x).iclass_of == Some(m) { return Ok(Value::True); } c = vm.heap.class(x).superclass; } Ok(Value::False) }),
        ("ancestors", Aspec::none(), |vm, s, _a, _b| { let mut list = vec![]; let mut c = Some(s.obj().unwrap()); while let Some(x) = c { let cd = vm.heap.class(x); if cd.origin.is_some() { c = cd.superclass; continue; } let shown = cd.iclass_of.or(cd.origin_of).unwrap_or(x); list.push(Value::Obj(shown)); c = cd.superclass; } Ok(vm.ary_new(list)) }),
        ("attr_reader", Aspec::any(), |vm, s, a, _b| { attr(vm, s, a, true, false) }),
        ("attr_writer", Aspec::any(), |vm, s, a, _b| { attr(vm, s, a, false, true) }),
        ("attr_accessor", Aspec::any(), |vm, s, a, _b| { attr(vm, s, a, true, true) }),
        ("attr", Aspec::any(), |vm, s, a, _b| { attr(vm, s, a, true, false) }),
        ("alias_method", Aspec::any(), |vm, s, a, _b| { argc!(vm, a, 2); let m = s.obj().unwrap(); let (new, old) = (sym_arg(vm, a[0])?, sym_arg(vm, a[1])?); vm.alias_method(m, new, old)?; Ok(s) }),
        // `mrb_mod_undef` (src/class.c) answers nil
        ("undef_method", Aspec::any(), |vm, s, a, _b| { let m = s.obj().unwrap(); for n in a { let n = sym_arg(vm, *n)?; vm.undef_method(m, n)?; } Ok(Value::Nil) }),
        // `mrb_mod_define_method_m` is `define_method_m(mrb, c, MRB_METHOD_PUBLIC_FL)`
        // (src/class.c): the visibility is written, not taken from the scope, so a
        // `define_method` under a bare `private` or `module_function` is still public — only
        // the `define_method` on main (`top_define_method`) asks for private, and there is no
        // such method here. `initialize` & co. are still caught by `Vm::def_method`. A block
        // keeps its own class (`mrb_define_method_raw` sets it only on a proc without an
        // environment), which is where its constants are read from (`Vm::proc_target_class`).
        // The method is a strict copy of the block ([`method_proc`]).
        ("define_method", Aspec::req(1) | Aspec::opt(1), |vm, s, a, b| { let m = s.obj().unwrap(); let n = sym_arg(vm, a[0])?; let body = if a.len() == 2 { a[1] } else { b }; match body {
            Value::Obj(p) if matches!(vm.heap.get(p).kind, ObjKind::Proc(_)) => { let p = method_proc(vm, p, m); vm.def_method(m, n, Method::Ruby(p), Vis::Public)?; Ok(Value::Sym(n)) }
            // CRuby accepts a Method/UnboundMethod: copy the method it names.
            Value::Obj(o) if matches!(vm.class_name(vm.real_class_of(Value::Obj(o))).as_str(), "Method" | "UnboundMethod") => {
                let owner = vm.ivar_get(o, "_owner");
                let name = vm.ivar_get(o, "_name");
                if let (Value::Obj(owner), Value::Sym(name)) = (owner, name) {
                    if let Some((method, _)) = vm.find_method(owner, name) {
                        vm.def_method(m, n, method, Vis::Public)?;
                        return Ok(Value::Sym(n));
                    }
                }
                let d = vm.describe_for_error(Value::Obj(o));
                Err(vm.raise_type(&format!("wrong argument type {d} (expected Proc)")))
            }
            Value::Nil if a.len() == 1 => Err(vm.raise_arg("no block given")),
            v => { let d = vm.describe_for_error(v); Err(vm.raise_type(&format!("wrong argument type {d} (expected Proc)"))) }
        } }),
        // `mrb_mod_method_defined` (src/class.c of mruby 4.1.0-rc2): one argument, and the answer
        // is `mrb_obj_respond_to` on the class — any method that is there and not "not
        // implemented", private ones included
        ("method_defined?", Aspec::req(1), |vm, s, a, _b| { let n = sym_arg(vm, a[0])?; let m = s.obj().unwrap(); Ok(Value::bool(match vm.find_method(m, n) { Some((Method::Native(f, _), _)) if vm.notimpl_fns.iter().any(|g| core::ptr::fn_addr_eq(*g, f)) => false, Some(_) => true, None => false })) }),
        ("public_method_defined?", Aspec::req(1) | Aspec::opt(1), |vm, s, a, _b| { let n = sym_arg(vm, a[0])?; let m = s.obj().unwrap(); Ok(Value::bool(match vm.find_method(m, n) { Some((_, owner)) => vm.method_vis(owner, n) == Vis::Public, None => false })) }),
        ("private_method_defined?", Aspec::req(1) | Aspec::opt(1), |vm, s, a, _b| { let n = sym_arg(vm, a[0])?; let m = s.obj().unwrap(); Ok(Value::bool(match vm.find_method(m, n) { Some((_, owner)) => vm.method_vis(owner, n) == Vis::Private, None => false })) }),
        ("protected_method_defined?", Aspec::req(1) | Aspec::opt(1), |vm, s, a, _b| { let n = sym_arg(vm, a[0])?; let m = s.obj().unwrap(); Ok(Value::bool(match vm.find_method(m, n) { Some((_, owner)) => vm.method_vis(owner, n) == Vis::Protected, None => false })) }),
        ("prepend", Aspec::any(), |vm, s, a, _b| { let cls = s.obj().unwrap(); for mv in a.iter().rev() { let m = module_arg(vm, *mv)?; vm.prepend_module(cls, m)?; let hook = vm.intern("prepended"); vm.funcall(Value::Obj(m), hook, &[s], Value::Nil)?; } Ok(s) }),
        ("instance_method", Aspec::req(1), |vm, _s, _a, _b| Err(vm.raise(vm.core.not_implemented_error, "instance_method is not implemented"))),
        ("const_get", Aspec::req(1), |vm, s, a, _b| {
            // `mrb_mod_const_get` (src/class.c of mruby 4.1.0-rc2): each name goes through
            // `mrb_const_get`, which does not look in Object for a class (`Foo.const_get(:String)`
            // is a NameError) and names a segment that is not a class or module with
            // `mod_const_check`'s message. "A::B" paths are resolved segment by segment.
            if let Some(path) = vm.str_bytes(a[0]).map(|b| String::from_utf8_lossy(b).into_owned()) {
                if path.contains("::") {
                    let mut cur = s;
                    for seg in path.split("::") {
                        if seg.is_empty() { return Err(vm.raise(vm.core.name_error, &format!("wrong constant name '{path}'"))); }
                        let n = vm.intern(seg);
                        check_const_name(vm, n)?;
                        let m = match cur { Value::Obj(o) if vm.heap.is_class(o) => o, _ => return Err(vm.raise_type(crate::vm::NON_CLASS_LOOKUP)) };
                        cur = match vm.const_get_scoped(m, n) { Some(v) => v, None => { let cm = vm.intern("const_missing"); vm.funcall(cur, cm, &[Value::Sym(n)], Value::Nil)? } };
                    }
                    return Ok(cur);
                }
            }
            // a name that is neither a Symbol nor a String: `mrb_ensure_string_type`
            if !matches!(a[0], Value::Sym(_)) && vm.str_bytes(a[0]).is_none() { return Err(vm.not_a_string(a[0])); }
            let n = sym_arg(vm, a[0])?;
            check_const_name(vm, n)?;
            match vm.const_get_scoped(s.obj().unwrap(), n) { Some(v) => Ok(v), None => { let cm = vm.intern("const_missing"); vm.funcall(s, cm, &[Value::Sym(n)], Value::Nil) } }
        }),
        ("const_set", Aspec::req(2), |vm, s, a, _b| { let n = sym_arg(vm, a[0])?; check_const_name(vm, n)?; let m = s.obj().unwrap(); if vm.heap.get(m).frozen { return Err(vm.frozen_error(s)); } vm.heap.class_mut(m).consts.insert(n, Slot::from(a[1])); if let Value::Obj(o) = a[1] { if vm.heap.is_class(o) && vm.heap.class(o).name.is_none() { vm.heap.class_mut(o).name = Some(n); vm.heap.class_mut(o).outer = Some(m); } } vm.const_added(m, n)?; Ok(a[1]) }),
        // `mrb_mod_const_defined`: "n|b"; with `inherit` the whole superclass chain
        // (`mrb_const_defined`), without it the receiver's own table only, except that Object
        // goes on up its chain (`const_defined_0`, `recurse` FALSE)
        ("const_defined?", Aspec::req(1) | Aspec::opt(1), |vm, s, a, _b| {
            let inherit = a.get(1).map(|v| v.truthy()).unwrap_or(true);
            if let Some(path) = vm.str_bytes(a[0]).map(|b| String::from_utf8_lossy(b).into_owned()) {
                let mut current = s;
                for segment in path.split("::").filter(|segment| !segment.is_empty()) {
                    let n = vm.intern(segment);
                    check_const_name(vm, n)?;
                    let Some(class) = current.obj() else { return Ok(Value::False); };
                    current = if inherit {
                        vm.const_get(class, n)
                    } else {
                        vm.const_get_scoped(class, n)
                    }
                    .unwrap_or(Value::Nil);
                    if current.is_nil() {
                        return Ok(Value::False);
                    }
                    if !matches!(current, Value::Obj(object) if vm.heap.is_class(object)) {
                        return Ok(Value::False);
                    }
                }
                return Ok(Value::True);
            }
            let n = sym_arg(vm, a[0])?;
            check_const_name(vm, n)?;
            let klass = s.obj().unwrap();
            let mut c = Some(klass);
            while let Some(x) = c {
                let cd = vm.heap.class(x);
                let tbl = match cd.iclass_of.or(cd.origin_of) { Some(m) => &vm.heap.class(m).consts, None => &cd.consts };
                if tbl.contains_key(&n) { return Ok(Value::True); }
                if !inherit && klass != vm.core.object { break; }
                c = cd.superclass;
            }
            Ok(Value::False)
        }),
        ("module_eval", Aspec::opt(3) | Aspec::block(), |vm, s, _a, b| { if b.is_nil() { return Err(vm.raise_arg("no block given")); } vm.exec_block_with_self(b, s, &[s], None) }),
        ("class_eval", Aspec::opt(3) | Aspec::block(), |vm, s, _a, b| { if b.is_nil() { return Err(vm.raise_arg("no block given")); } vm.exec_block_with_self(b, s, &[s], None) }),
    ]);
    // The `MRB_MT_PRIVATE` half of `mod_rom_entries` (src/class.c). `method_removed` is *not*
    // among them here although the core table marks it private: mruby-metaprog defines it a
    // second time (`metaprog_mod_rom_entries`, `mrb_f_nil`) without the flag, and that second
    // definition is the one the reference ends up with. It is in `ext_metaprog.rs`, public,
    // next to the `remove_method` that fires it.
    vm.define_private_methods_args(c.module, &[
        ("included", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("extended", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("prepended", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("method_added", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("method_undefined", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("const_added", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        ("public", Aspec::any(), |vm, s, a, _b| set_vis(vm, s, a, Vis::Public)),
        ("private", Aspec::any(), |vm, s, a, _b| set_vis(vm, s, a, Vis::Private)),
        ("protected", Aspec::any(), |vm, s, a, _b| set_vis(vm, s, a, Vis::Protected)),
        ("remove_const", Aspec::req(1), |vm, s, a, _b| { let n = sym_arg(vm, a[0])?; check_const_name(vm, n)?; let m = s.obj().unwrap(); if vm.heap.get(m).frozen { return Err(vm.frozen_error(s)); } match vm.heap.class_mut(m).consts.remove(&n) { Some(v) => Ok(v.get()), None => { let nn = vm.sym_name(n); Err(vm.raise(vm.core.name_error, &format!("constant {nn} not defined"))) } } }),
        ("module_function", Aspec::any(), |vm, s, a, _b| {
            let m = s.obj().unwrap();
            // `mrb_mod_module_function` answers the module, with names or without
            if a.is_empty() { vm.set_scope_vis(Vis::Private, true); return Ok(s); }
            // one name at a time, as `mrb_mod_visibility`
            for v in a {
                let n = &vis_syms(vm, core::slice::from_ref(v))?[0];
                match vm.find_method(m, *n) {
                    Some((mth, _)) => { let sc = vm.singleton_class(s)?; vm.def_method(sc, *n, mth, Vis::Public)?; vm.set_visibility(m, *n, Vis::Private)?; }
                    // `mrb_method_search` (src/class.c): the class is not quoted there
                    None => { let nn = vm.sym_name(*n); let cn = module_str(vm, m); return Err(vm.raise(vm.core.name_error, &format!("undefined method '{nn}' for class {cn}"))); }
                }
            }
            Ok(s)
        }),
    ]);
    // Class
    vm.define_methods_args(c.class, &[
        // `Class#new` is bytecode in the reference (`new_iseq` of src/class.c): it sends
        // `allocate` and then `initialize`, so a class that redefines `allocate` decides
        // what its `new` makes
        ("new", Aspec::any(), |vm, s, a, b| {
            let c = match s.obj() { Some(c) => c, None => return Err(vm.raise_type("not a class")) };
            let alloc = vm.intern("allocate");
            let obj = match vm.find_method(vm.class_of(s), alloc) {
                // the built-in allocator: no need to go through a dispatch
                Some((crate::object::Method::Native(f, _), _)) if core::ptr::fn_addr_eq(f, default_allocate as crate::object::NativeFn) => vm.instance_alloc(c)?,
                Some(_) => vm.funcall(s, alloc, &[], Value::Nil)?,
                None => vm.instance_alloc(c)?,
            };
            let init = vm.s.initialize;
            // Called by a SEND, an `initialize` written in Ruby runs in a frame of its own that
            // answers the object whatever it returns (`Cci::KeepSelf`), which is what the
            // reference's bytecode `new` does (`new_iseq`: `SSENDB :initialize` then
            // `RETURN R0`), so it is no native boundary. A native `initialize` given a block is
            // called keeping the frame too, so that one which hands its loop to a frame
            // (`Array.new(n) { }`) can do it there, above a frame that answers the object.
            // one lookup, through the method cache, answers both what `initialize` is and
            // whether there is one (`respond_to`)
            match vm.method_ref_of(obj, init) {
                Some((crate::object::MethodRef::Ruby(p), owner)) if vm.in_frame() => {
                    let kw = match (vm.pending_kw, a.last()) { (Some(k), Some(l)) if !k.is_nil() && k == *l => Some(k), _ => None };
                    let pos = if kw.is_some() { &a[..a.len() - 1] } else { a };
                    vm.push_method_frame(p, owner, obj, init, pos, kw, b, false)?;
                    // it answers the object whatever `initialize` returns
                    vm.keep_self();
                    Ok(obj)
                }
                Some((crate::object::MethodRef::Native(f, _), _)) if vm.notimpl_fns.iter().any(|g| core::ptr::fn_addr_eq(*g, f)) => {
                    if !a.is_empty() { return Err(vm.argnum_error(a.len(), "0")); }
                    Ok(obj)
                }
                None => {
                    if !a.is_empty() { return Err(vm.argnum_error(a.len(), "0")); }
                    Ok(obj)
                }
                Some(_) if vm.in_frame() && !b.is_nil() => {
                    let kw = match (vm.pending_kw, a.last()) { (Some(k), Some(l)) if !k.is_nil() && k == *l => Some(k), _ => None };
                    vm.send_in_frame_then(obj, obj, init, a, kw, b)
                }
                Some(_) => { vm.funcall(obj, init, a, b)?; Ok(obj) }
            }
        }),
        ("allocate", Aspec::none(), default_allocate),
        ("superclass", Aspec::none(), |vm, s, _a, _b| { let mut c = vm.heap.class(s.obj().unwrap()).superclass; while let Some(x) = c { let cd = vm.heap.class(x); if cd.iclass_of.is_none() && !cd.is_singleton { return Ok(Value::Obj(x)); } c = cd.superclass; } Ok(Value::Nil) }),
    ]);
    // `cls_rom_entries` (src/class.c) marks `inherited` `MRB_MT_PRIVATE`
    vm.define_private_methods_args(c.class, &[
        ("inherited", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::Nil)),
        // `mrb_class_initialize` (src/class.c): "|C&", yields the block with the class as self
        // and answers the class. Declared `MRB_ARGS_OPT(1)` (`cls_rom_entries`), no block flag:
        // `Method#parameters` shows it (rc2-sweep-fixes Y6b)
        ("initialize", Aspec::opt(1), |vm, s, a, b| {
            if let Some(&v) = a.first() { if !matches!(v, Value::Obj(o) if vm.heap.is_class(o)) { return Err(not_class_module(vm, v)); } }
            if !b.is_nil() { return vm.exec_block_with_self_then(b, s, &[s], s); }
            Ok(s)
        }),
    ]);
    // `mrb_mod_initialize` (src/class.c): the same with no argument, declared `MRB_ARGS_NONE()`
    vm.define_private_methods_args(c.module, &[
        ("initialize", Aspec::none(), |vm, s, _a, b| {
            if !b.is_nil() { return vm.exec_block_with_self_then(b, s, &[s], s); }
            Ok(s)
        }),
    ]);
    // `mrb_init_class` (src/class.c) undefines `module_function` in Class: a class answers
    // NoMethodError for it, a module keeps Module's
    let mf = vm.intern("module_function");
    let _ = vm.undef_method(c.class, mf);
    // Class.new (with optional superclass) on its singleton class (`mrb_class_new_class`);
    // `Module.new` is `Class#new` (`InstanceKind::Module`, then `Module#initialize`)
    let sc = vm.singleton_class(Value::Obj(c.class)).unwrap();
    vm.define_method_args(sc, "new", Aspec::opt(1) | Aspec::block(), |vm, _s, a, b| {
        // "|C&" (`ensure_class_type`), then `mrb_class_new`'s `mrb_check_inheritable`
        let sup = if a.is_empty() { vm.core.object } else { check_inheritable(vm, a[0])? };
        let cls = vm.heap.alloc(vm.core.class, ObjKind::Class(crate::object::ClassData { superclass: Some(sup), ..Default::default() }));
        vm.singleton_class(Value::Obj(cls))?;
        vm.call_inherited(sup, cls)?;
        // called by a SEND the block runs in a frame of its own (`mrb_class_initialize` yields)
        if !b.is_nil() { return vm.exec_block_with_self_then(b, Value::Obj(cls), &[Value::Obj(cls)], Value::Obj(cls)); }
        Ok(Value::Obj(cls))
    });
    // main (the top-level self) prints as "main"
    let ts = Value::Obj(vm.top_self);
    let msc = vm.singleton_class(ts).unwrap();
    vm.define_methods_args(msc, &[("to_s", Aspec::none(), |vm, _s, _a, _b| Ok(vm.str_new(b"main"))), ("inspect", Aspec::none(), |vm, _s, _a, _b| Ok(vm.str_new(b"main")))]);
    // `top_public`/`top_private`/`top_protected` (src/class.c): `mrb_mod_visibility` on Object,
    // answering Object
    vm.define_methods_args(msc, &[
        ("public", Aspec::any(), |vm, _s, a, _b| { let o = Value::Obj(vm.core.object); set_vis(vm, o, a, Vis::Public) }),
        ("private", Aspec::any(), |vm, _s, a, _b| { let o = Value::Obj(vm.core.object); set_vis(vm, o, a, Vis::Private) }),
        ("protected", Aspec::any(), |vm, _s, a, _b| { let o = Value::Obj(vm.core.object); set_vis(vm, o, a, Vis::Protected) }),
    ]);
    // Version constants (src/version.c). RUBY_ENGINE stays "mruby": the reference's own tests
    // branch on it (test/assert.rb, mruby-fiber's fiber2.rb) to tell mruby from CRuby, and so
    // may any script written for mruby. What tells SabiRuby apart is MRUBY_PLATFORM, and
    // SABIRUBY_VERSION (this crate's version), which the reference does not define at all —
    // `defined?(SABIRUBY_VERSION)` is the one-line "am I on SabiRuby?" (docs/design/gems.md).
    for (name, v) in [("RUBY_VERSION", "4.1"), ("RUBY_ENGINE", "mruby"), ("RUBY_ENGINE_VERSION", "4.1.0"), ("MRUBY_VERSION", "4.1.0"),
                      ("MRUBY_PLATFORM", "rust-sabiruby"), ("SABIRUBY_VERSION", env!("CARGO_PKG_VERSION")), ("MRUBY_RELEASE_DATE", "2026-09-11"),
                      // the commit this VM was built from (`build.rs`); `HEAD` where there was no git,
                      // which is the reference's own default too
                      ("MRUBY_REVISION", crate::REVISION),
                      ("MRUBY_DESCRIPTION", "mruby 4.1.0RC2 (2026-09-11)"), ("MRUBY_COPYRIGHT", "mruby - Copyright (c) 2010-2026 mruby developers")] {
        let s = vm.str_new(v.as_bytes());
        if let Some(o) = s.obj() { vm.heap.get_mut(o).frozen = true; }
        let n = vm.intern(name);
        vm.heap.class_mut(c.object).consts.insert(n, Slot::from(s));
    }
    // GC module (docs/design/gc.md): `start`, `enable`/`disable`, `interval_ratio` and `malloc_threshold` drive the
    // collector; the incremental/generational knobs are kept as values only.
    let gc = vm.define_module("GC");
    let gsc = vm.singleton_class(Value::Obj(gc)).unwrap();
    vm.define_methods_args(gsc, &[
        ("start", Aspec::none(), |vm, _s, _a, _b| { vm.gc_start(); Ok(Value::Nil) }),
        ("enable", Aspec::none(), |vm, _s, _a, _b| { let old = vm.gc_disabled; vm.gc_disabled = false; Ok(Value::bool(old)) }),
        ("disable", Aspec::none(), |vm, _s, _a, _b| { let old = vm.gc_disabled; vm.gc_disabled = true; Ok(Value::bool(old)) }),
        ("interval_ratio", Aspec::none(), |vm, _s, _a, _b| Ok(Value::Int(vm.gc_interval_ratio))),
        ("interval_ratio=", Aspec::req(1), |vm, _s, a, _b| { let r = vm.expect_int(a[0], "ratio")?; vm.gc_interval_ratio = r; Ok(Value::Int(r)) }),
        ("step_ratio", Aspec::none(), |_vm, _s, _a, _b| Ok(Value::Int(200))),
        ("step_ratio=", Aspec::req(1), |vm, _s, a, _b| { let r = vm.expect_int(a[0], "ratio")?; if r <= 0 { return Err(vm.raise_arg("step_ratio must be positive")); } Ok(Value::Nil) }),
        ("step_limit", Aspec::none(), |vm, _s, _a, _b| Ok(Value::Int(vm.gc_step_limit))),
        ("step_limit=", Aspec::req(1), |vm, _s, a, _b| { let r = vm.expect_int(a[0], "limit")?; if r < 0 { return Err(vm.raise_arg("step_limit must be non-negative")); } vm.gc_step_limit = r; Ok(Value::Int(r)) }),
        ("generational_mode", Aspec::none(), |_vm, _s, _a, _b| Ok(Value::False)),
        ("generational_mode=", Aspec::req(1), |vm, _s, a, _b| {
            // mruby-task: a generational minor cycle is one atomic step, which would defeat a
            // collector the scheduler drives (`gc_scheduler_driven_set`)
            if vm.task.gc_driven && a.first().map(|v| v.truthy()).unwrap_or(false) {
                return Err(vm.raise(vm.core.runtime_error, "generational mode cannot be enabled while GC is scheduler-driven"));
            }
            Ok(a.first().copied().unwrap_or(Value::Nil))
        }),
        // mruby-task's `GC.scheduler_driven` family (`mrbgems/mruby-task/src/gc.c`): the
        // scheduler collects from its idle points instead of the allocation path
        ("scheduler_driven", Aspec::any(), |vm, _s, _a, _b| Ok(Value::bool(vm.task.gc_driven))),
        ("scheduler_driven=", Aspec::req(1), |vm, _s, a, _b| {
            let on = a[0].truthy();
            if on && vm.gc_disabled { return Err(vm.raise(vm.core.runtime_error, "cannot enable scheduler-driven GC while GC is disabled")); }
            vm.task.gc_driven = on;
            Ok(Value::bool(on))
        }),
        ("debt_limit", Aspec::any(), |vm, _s, _a, _b| Ok(Value::Int(vm.task.gc_debt_limit))),
        ("debt_limit=", Aspec::req(1), |vm, _s, a, _b| { let n = vm.expect_int(a[0], "limit")?; if n < 0 { return Err(vm.raise_arg("debt_limit must be non-negative")); } vm.task.gc_debt_limit = n; Ok(Value::Int(n)) }),
        ("malloc_threshold", Aspec::none(), |vm, _s, _a, _b| Ok(Value::Int(vm.heap.malloc_threshold as i64))),
        ("malloc_threshold=", Aspec::req(1), |vm, _s, a, _b| { let r = vm.expect_int(a[0], "threshold")?; if r < 0 { return Err(vm.raise_arg("malloc_threshold must be non-negative")); } vm.heap.malloc_threshold = r as usize; vm.heap.malloc_limit = r as usize; Ok(Value::Int(r)) }),
        ("stat", Aspec::none(), |vm, _s, _a, _b| { let h = vm.hash_new(); let live = vm.heap.live_count() as i64; let (mi, mt) = (vm.heap.malloc_increase as i64, vm.heap.malloc_threshold as i64); for (k, v) in [("live", live), ("debt", 0), ("state", 0), ("generational", 0), ("full", 0), ("step_limit", vm.gc_step_limit), ("malloc_increase", mi), ("malloc_threshold", mt), ("symbol_count", vm.syms.len() as i64), ("dynamic_symbol_count", 0)] { let ks = Value::Sym(vm.intern(k)); vm.hash_set(h, ks, Value::Int(v))?; } Ok(h) }),
    ]);
    let n = vm.intern("MRUBY_RELEASE_NO");
    vm.heap.class_mut(c.object).consts.insert(n, Slot::from(Value::Int(40100)));

    // nil / true / false
    vm.define_methods_args(c.nil_class, &[
        ("to_s", Aspec::none(), |vm, _s, _a, _b| Ok(vm.str_new(b""))),
        ("inspect", Aspec::none(), |vm, _s, _a, _b| Ok(vm.str_new(b"nil"))),
        // `nil_rom_entries` (src/object.c): `mrb_true`; Kernel's answers false for the rest
        ("nil?", Aspec::none(), |_vm, _s, _a, _b| Ok(Value::True)),
        // to_a/to_h/to_i/to_f are mruby-object-ext (`ext_object.rs`)
        // `nil_match`: nil matches nothing, and it is the only object core gives `=~` to
        ("=~", Aspec::req(1), |_vm, _s, _a, _b| { Ok(Value::Nil) }),
        ("&", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::False)),
        ("|", Aspec::req(1), |_vm, _s, a, _b| Ok(Value::bool(a.first().map(|v| v.truthy()).unwrap_or(false)))),
        ("^", Aspec::req(1), |_vm, _s, a, _b| Ok(Value::bool(a.first().map(|v| v.truthy()).unwrap_or(false)))),
    ]);
    vm.define_methods_args(c.true_class, &[
        ("to_s", Aspec::none(), |vm, _s, _a, _b| Ok(vm.str_new(b"true"))),
        ("inspect", Aspec::none(), |vm, _s, _a, _b| Ok(vm.str_new(b"true"))),
        ("&", Aspec::req(1), |_vm, _s, a, _b| Ok(Value::bool(a.first().map(|v| v.truthy()).unwrap_or(false)))),
        ("|", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::True)),
        ("^", Aspec::req(1), |_vm, _s, a, _b| Ok(Value::bool(!a.first().map(|v| v.truthy()).unwrap_or(false)))),
    ]);
    vm.define_methods_args(c.false_class, &[
        ("to_s", Aspec::none(), |vm, _s, _a, _b| Ok(vm.str_new(b"false"))),
        ("inspect", Aspec::none(), |vm, _s, _a, _b| Ok(vm.str_new(b"false"))),
        ("&", Aspec::req(1), |_vm, _s, _a, _b| Ok(Value::False)),
        ("|", Aspec::req(1), |_vm, _s, a, _b| Ok(Value::bool(a.first().map(|v| v.truthy()).unwrap_or(false)))),
        ("^", Aspec::req(1), |_vm, _s, a, _b| Ok(Value::bool(a.first().map(|v| v.truthy()).unwrap_or(false)))),
    ]);
    // Comparable / Enumerable bodies come from mrblib (`clamp` is mruby-compar-ext's Ruby).
}

/// `equal?`: identity; Floats compare bit for bit (mruby reads the boxed representation).
pub fn same_object(a: Value, b: Value) -> bool {
    match (a, b) { (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits(), _ => a == b }
}

pub fn object_id(v: Value) -> Value {
    match v {
        Value::Nil => Value::Int(8),
        Value::True => Value::Int(20),
        Value::False => Value::Int(0),
        Value::Int(i) => Value::Int(i.wrapping_mul(2).wrapping_add(1)),
        Value::Float(f) => Value::Int(f.to_bits() as i64),
        Value::Sym(s) => Value::Int((s.0 as i64) << 8 | 0x0c),
        Value::Obj(o) => Value::Int((o.0 as i64 + 1) * 8),
    }
}

/// `#<ClassName:0x...>` — the address is the heap index, so it is stable.
pub fn any_to_s(vm: &mut Vm, v: Value) -> String {
    let c = vm.real_class_of(v);
    let cn = vm.class_name(c);
    match v {
        Value::Obj(o) => format!("#<{cn}:0x{:012x}>", (o.0 as usize + 1) * 0x40),
        _ => format!("#<{cn}>"),
    }
}

fn obj_inspect(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    if let Value::Obj(o) = s {
        let ivars = vm.heap.get(o).ivars.clone();
        // `mrb_obj_iv_inspect` is for an OBJECT; a Random's or a Rational's own bookkeeping is not shown
        if !ivars.is_empty() && matches!(vm.heap.get(o).kind, ObjKind::Object) && vm.heap.get(o).iv_p() {
            let mut out = any_to_s(vm, s);
            out.pop(); // '>'
            let mut first = true;
            for (k, v) in ivars {
                out.push_str(if first { " " } else { ", " });
                first = false;
                out.push_str(&vm.sym_name(k));
                out.push('=');
                out.push_str(&vm.inspect_str(v.get())?);
            }
            out.push('>');
            return Ok(vm.str_from(out));
        }
    }
    let t = any_to_s(vm, s);
    Ok(vm.str_from(t))
}

pub(crate) fn mod_to_s(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    let t = module_str(vm, s.obj().unwrap());
    Ok(vm.str_from(t))
}

/// What `Module#to_s` answers for a class or module, as a Rust string: also how the
/// reference's messages spell a class (`%C` and `%v` in `mrb_vformat` go through
/// `mrb_obj_as_string`), so a singleton class reads `#<Class:Foo>` there too, and one of a
/// singleton class `#<Class:#<Class:Foo>>`.
pub(crate) fn module_str(vm: &mut Vm, o: crate::value::ObjId) -> String {
    let cd = vm.heap.class(o);
    if cd.is_singleton {
        let att = cd.attached.map(|s| s.get()).unwrap_or(Value::Nil);
        let inner = match att { Value::Obj(x) if vm.heap.is_class(x) => module_str(vm, x), _ => any_to_s(vm, att) };
        return format!("#<Class:{inner}>");
    }
    if cd.name.is_none() && cd.iclass_of.is_none() {
        return any_to_s(vm, Value::Obj(o));
    }
    vm.class_name(o)
}

/// `NameError` unless `n` looks like a constant name (uppercase first letter).
pub fn check_const_name(vm: &mut Vm, n: crate::symbol::Sym) -> VmResult<()> {
    let name = vm.sym_name(n);
    let ok = name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) && name.chars().all(|c| c == '_' || c.is_alphanumeric() || !c.is_ascii());
    if ok { Ok(()) } else { Err(vm.raise(vm.core.name_error, &format!("wrong constant name {name}"))) }
}

/// `Kernel#clone`: like dup, but keeps the singleton class and the frozen state.
fn clone(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    obj_clone(vm, s)
}

/// `mrb_obj_clone`: `dup`, plus the singleton class and the frozen state.
pub(crate) fn obj_clone(vm: &mut Vm, s: Value) -> VmResult<Value> {
    let o = match s { Value::Obj(o) => o, _ => return Ok(s) };
    if matches!(vm.heap.get(o).kind, ObjKind::Data { .. }) {
        let n = { let c = vm.real_class_of(s); vm.class_name(c) };
        return Err(vm.raise_type(&format!("can't clone {n}")));
    }
    let d = dup(vm, s, &[], Value::Nil)?;
    let Value::Obj(n) = d else { return Ok(d) };
    if n == o { return Ok(d); }
    let cur = vm.heap.get(o).class;
    if vm.heap.class(cur).is_singleton {
        let methods = vm.heap.class(cur).methods.clone();
        let sup = vm.heap.class(cur).superclass;
        let sc = vm.singleton_class(d)?;
        vm.heap.class_mut(sc).methods = methods;
        // keep modules extended into the original
        vm.heap.class_mut(sc).superclass = sup;
    }
    let frozen = vm.heap.get(o).frozen;
    vm.heap.get_mut(n).frozen = frozen;
    Ok(d)
}

/// `ensure_class_type` (src/class.c): "%v is not class/module", `%v` being `to_s`.
#[cold]
pub(crate) fn not_class_module(vm: &mut Vm, v: Value) -> crate::error::VmError {
    let d = match vm.as_string(v) { Ok(d) => d, Err(e) => return e };
    let mut m = d; m.extend_from_slice(b" is not class/module");
    vm.raise_type(&String::from_utf8_lossy(&m))
}

/// `mrb_check_inheritable` (src/class.c) after `C`: a module or a singleton class is "superclass
/// must be a Class (%C given)", `Class` itself cannot be subclassed.
fn check_inheritable(vm: &mut Vm, v: Value) -> VmResult<crate::value::ObjId> {
    let o = class_arg(vm, v)?;
    if vm.heap.class(o).is_module || vm.heap.class(o).is_singleton {
        let d = vm.as_string(v)?;
        let d = String::from_utf8_lossy(&d).into_owned();
        return Err(vm.raise_type(&format!("superclass must be a Class ({d} given)")));
    }
    if o == vm.core.class { return Err(cant_subclass_class(vm)); }
    Ok(o)
}

/// `mrb_check_inheritable`'s refusal of `Class`.
#[cold]
#[inline(never)]
pub(crate) fn cant_subclass_class(vm: &mut Vm) -> crate::error::VmError {
    vm.raise_type("can't make subclass of Class")
}

/// `mrb_get_args`' `c` and `C` (src/class.c): a class or a module, or `ensure_class_type`'s
/// TypeError "%v is not class/module".
pub fn class_arg(vm: &mut Vm, v: Value) -> VmResult<crate::value::ObjId> {
    match v {
        Value::Obj(o) if vm.heap.is_class(o) => Ok(o),
        _ => Err(not_class_module(vm, v)),
    }
}

/// `mrb_check_type(mrb, m, MRB_TT_MODULE)`, what `include`, `prepend`, `extend` and `include?`
/// ask of each module: a class is refused as any other value is ("wrong argument type Class
/// (expected Module)").
pub(crate) fn module_arg(vm: &mut Vm, v: Value) -> VmResult<crate::value::ObjId> {
    match v {
        Value::Obj(o) if vm.heap.is_class(o) && vm.heap.class(o).is_module => Ok(o),
        _ => Err(super::ext_array::wrong_type(vm, v, "Module")),
    }
}

pub fn sym_arg(vm: &mut Vm, v: Value) -> VmResult<crate::symbol::Sym> {
    match v {
        Value::Sym(s) => Ok(s),
        _ => match vm.str_bytes(v) {
            Some(b) => { let b = b.to_vec(); Ok(vm.syms.intern(&b)) }
            None => { let d = vm.inspect_str(v)?; Err(vm.raise_type(&format!("{d} is not a symbol nor a string"))) }
        },
    }
}

fn is_a(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let c = class_arg(vm, a[0])?;
    Ok(Value::bool(vm.obj_is_kind_of(s, c)))
}


/// `Kernel#send` / `__send__` when reached from native code. From bytecode the
/// VM redirects in place instead (`Vm::op_send_redirect`), which is why this
/// must stay a plain fn: the VM recognises it by address.
pub fn send(vm: &mut Vm, s: Value, a: &[Value], b: Value) -> VmResult<Value> {
    if a.is_empty() { return Err(vm.argnum_error(0, "1+")); }
    let m = sym_arg(vm, a[0])?;
    // reached by a native that keeps the frame (`Method#call` of `method(:send)`), the named
    // method still runs in it
    let kw = match (vm.pending_kw, a.last()) { (Some(k), Some(l)) if a.len() > 1 && !k.is_nil() && k == *l => Some(k), _ => None };
    vm.send_in_frame(s, m, &a[1..], kw, b)
}

/// `mrb_obj_missing` (src/class.c): "n*!" (a name that is a Symbol or a String, at least one
/// argument), then `mrb_method_missing`'s NoMethodError with the name and the rest as `args`.
fn method_missing(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let Some(&first) = a.first() else { return Err(vm.argnum_error(0, "1+")) };
    let mid = sym_arg(vm, first)?;
    let name = vm.sym_name(mid);
    let desc = vm.describe_for_error(s);
    let e = vm.no_method_error(mid, s, &format!("undefined method '{name}' for {desc}"));
    if let crate::error::VmError::Raise(Value::Obj(o)) = e { let k = vm.intern("@args"); let rest = vm.ary_new(a[1..].to_vec()); vm.heap.ivar_set(o, k, rest); }
    Err(e)
}

fn dup(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    let o = match s { Value::Obj(o) => o, _ => return Ok(s) };
    let (class, ivars, kind) = {
        let h = vm.heap.get(o);
        let kind = match &h.kind {
            ObjKind::Object => ObjKind::Object,
            // a String and an Array are filled by `initialize_copy` below (`init_copy` copies
            // neither; `mrb_str_replace`, `mrb_ary_replace_m`), so they start empty
            ObjKind::String(_) => ObjKind::String(Default::default()),
            ObjKind::Array(_) => ObjKind::Array(Default::default()),
            ObjKind::Hash(hd) => ObjKind::Hash(hd.clone()),
            ObjKind::Range { .. } => ObjKind::Object, // `initialize_copy` fills the copy in, as the reference's `range_initialize_copy` does
            ObjKind::Exception => ObjKind::Exception,
            // a Regexp copies its source and flags and compiles its own pattern
            // (`regexp_init_copy`), which `initialize_copy` does below; a MatchData is never
            // copied, `dup` on one answering a plain object as the reference's does
            #[cfg(feature = "regexp")]
            ObjKind::Regexp(_) | ObjKind::MatchData { .. } => ObjKind::Object,
            // a handle names one value the host owns, and the free hook fires once per object
            // that carries it: copying it would tell the host to drop that value twice, and
            // the second object would go on naming what is no longer there. Only the host
            // knows how to copy its own value, so only the host can define `dup` (a closure
            // that allocates a new one and calls `Vm::data_new` again).
            ObjKind::Data { .. } => { let n = { let c = vm.real_class_of(s); vm.class_name(c) }; return Err(vm.raise_type(&format!("can't dup {n}"))); }
            ObjKind::BigInt(b) => ObjKind::BigInt(b.clone()),
            ObjKind::Class(cd) => {
                let (hc, data, is_module, ivars) = (h.class, crate::object::ClassData { name: None, superclass: cd.superclass, methods: cd.methods.clone(), vis: cd.vis.clone(), consts: cd.consts.clone(), cvars: cd.cvars.clone(), is_module: cd.is_module, instance_kind: cd.instance_kind, outer: None, ..Default::default() }, cd.is_module, h.ivars.clone());
                // an origin's table belongs to the copy too
                let data = match cd.origin { Some(org) => { let o = vm.heap.class(org); crate::object::ClassData { methods: o.methods.clone(), vis: o.vis.clone(), superclass: o.superclass, ..data } } None => data };
                let n = vm.heap.alloc(hc, ObjKind::Class(data));
                vm.heap.get_mut(n).ivars = ivars;
                // prepended modules of the original are prepended to the copy too (farthest first)
                if vm.heap.class(o).origin.is_some() {
                    let origin = vm.heap.class(o).origin.unwrap();
                    let mut mods = vec![];
                    let mut cc = vm.heap.class(o).superclass;
                    while let Some(x) = cc { if x == origin { break; } if let Some(m) = vm.heap.class(x).iclass_of { mods.push(m); } cc = vm.heap.class(x).superclass; }
                    for m in mods.iter().rev() { vm.prepend_module(n, *m)?; }
                }
                // singleton methods (class methods) come along
                let cur = vm.heap.get(o).class;
                if vm.heap.class(cur).is_singleton {
                    let (methods, vis) = { let c = vm.heap.class(cur); (c.methods.clone(), c.vis.clone()) };
                    let sc = vm.singleton_class(Value::Obj(n))?;
                    vm.heap.class_mut(sc).methods = methods;
                    vm.heap.class_mut(sc).vis = vis;
                } else if !is_module { vm.singleton_class(Value::Obj(n))?; }
                return Ok(Value::Obj(n));
            }
            // a Proc copies its body and is an orphan (`mrb_proc_copy`, then `MRB_PROC_ORPHAN`
            // in `mrb_proc_init_copy`, which `initialize_copy` below is)
            ObjKind::Proc(p) => ObjKind::Proc(crate::object::ProcData { irep: p.irep, upper: p.upper, env: p.env, target_class: p.target_class, strict: p.strict, scope: p.scope, loose_arity: p.loose_arity, orphan: true, mid: None }),
            ObjKind::Env(_) | ObjKind::Break { .. } | ObjKind::Fiber(_) | ObjKind::Task(_) => return Ok(s),
        };
        (vm.real_class_of(s), h.ivars.clone(), kind)
    };
    let n = vm.heap.alloc(class, kind);
    vm.heap.get_mut(n).ivars = ivars;
    // a copy of a Set, a Random, ... cannot hold instance variables either
    vm.heap.get_mut(n).no_ivars = vm.heap.get(o).no_ivars;
    // a copy of a Struct or a Data is one too, not an Array
    vm.heap.get_mut(n).tt_struct = vm.heap.get(o).tt_struct;
    // a copy of a String is read the way the original is (`RSTR_ENC_CR_COPY` in `mrb_str_dup`)
    vm.heap.get_mut(n).binary = vm.heap.get(o).binary;
    // mruby `init_copy`: the copy gets `initialize_copy(original)` (Ruby overrides may refuse)
    let ic = vm.intern("initialize_copy");
    vm.funcall(Value::Obj(n), ic, &[s], Value::Nil)?;
    Ok(Value::Obj(n))
}

/// `mod_attr_define` (src/class.c of mruby 4.1.0-rc2), once for the readers and once more for
/// the writers of `attr_accessor`: the instance variable's name is checked as
/// `mrb_iv_name_sym_check` checks it, and each method goes in through `mrb_define_method_raw`
/// from a native's frame — so it is public whatever `private` or `module_function` the caller
/// is under (the scope `find_visibility_scope` finds is the native's own), `method_added` is not
/// fired, and the answer is nil.
fn attr(vm: &mut Vm, s: Value, a: &[Value], reader: bool, writer: bool) -> VmResult<Value> {
    let cls = s.obj().unwrap();
    for (on, w) in [(reader, false), (writer, true)] {
        if !on { continue; }
        for n in a {
            let n = sym_arg(vm, *n)?;
            let name = vm.sym_name(n);
            let iv = vm.intern(&format!("@{name}"));
            if !super::ext_metaprog::iv_name_p(vm.syms.name(iv)) {
                let ivn = vm.sym_name(iv);
                return Err(vm.name_error(iv, &format!("'{ivn}' is not allowed as an instance variable name")));
            }
            if w {
                let wn = vm.intern(&format!("{name}="));
                vm.def_method_nohook(cls, wn, Method::AttrWriter(iv), Vis::Public)?;
            } else {
                vm.def_method_nohook(cls, n, Method::AttrReader(iv), Vis::Public)?;
            }
        }
    }
    Ok(Value::Nil)
}

/// The names `mrb_mod_visibility` and `mrb_mod_module_function` (src/class.c of mruby
/// 4.1.0-rc2) take: each one a Symbol (`mrb_check_type(..., MRB_TT_SYMBOL)`), so an Array or a
/// String is a TypeError there.
fn vis_syms(vm: &mut Vm, a: &[Value]) -> VmResult<Vec<crate::symbol::Sym>> {
    let mut out = vec![];
    for v in a {
        match v { Value::Sym(n) => out.push(*n), _ => return Err(super::ext_array::wrong_type(vm, *v, "Symbol")) }
    }
    Ok(out)
}

/// `public`/`private`/`protected` (`mrb_mod_visibility`): with names sets them, without
/// changes the frame default. Either way the answer is the module (`mrb_mod_private` returns
/// `mod`).
fn set_vis(vm: &mut Vm, s: Value, a: &[Value], vis: Vis) -> VmResult<Value> {
    let m = s.obj().unwrap();
    if a.is_empty() {
        vm.set_scope_vis(vis, false);
        return Ok(s);
    }
    // one name at a time, its type and then the change (a later bad name leaves the earlier
    // ones changed, and an undefined name before a bad one is the NameError)
    for v in a { let n = vis_syms(vm, core::slice::from_ref(v))?[0]; vm.set_visibility(m, n, vis)?; }
    Ok(s)
}

impl Vm {
    /// Deterministic hash used by `Object#hash` and Hash keys.
    pub fn value_hash(&self, v: Value) -> i64 {
        fn fnv(bytes: &[u8]) -> i64 {
            let mut h: u64 = 0xcbf29ce484222325;
            for b in bytes { h ^= *b as u64; h = h.wrapping_mul(0x100000001b3); }
            (h >> 1) as i64
        }
        match v {
            Value::Obj(o) => match &self.heap.get(o).kind {
                ObjKind::String(b) => fnv(b),
                // `mrb_bint_hash`: by value, so a wide integer works as a Hash key
                ObjKind::BigInt(b) => fnv(&b.hash_bytes()),
                // by the handle, to agree with `==`: two objects naming the same host value
                // are one key in a Hash (`Vm::data_new`)
                ObjKind::Data { tag, handle } => {
                    let mut bytes = [0u8; 12];
                    bytes[..4].copy_from_slice(&tag.to_le_bytes());
                    bytes[4..].copy_from_slice(&handle.to_le_bytes());
                    fnv(&bytes)
                }
                _ => (o.0 as i64 + 1) * 8,
            },
            Value::Int(i) => i,
            Value::Float(f) => f.to_bits() as i64 >> 1,
            Value::Sym(s) => s.0 as i64 * 31 + 7,
            Value::Nil => 8,
            Value::True => 20,
            Value::False => 0,
        }
    }
}
