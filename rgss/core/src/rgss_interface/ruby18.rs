//! Ruby 1.8 behaviour for projects that run on the stock RGSS1 player
//! ([`Project::ruby18`]).
//!
//! RPG Maker XP embeds Ruby 1.8, and its games' scripts rely on a few answers
//! that later Rubies changed. Only those observable differences are
//! reproduced here, and only for such projects; mkxp-z games (Ruby 3) keep
//! the modern behaviour. Syntax that 1.9 removed is handled where scripts are
//! compiled (`compile_rgss`).

use super::*;

/// The `String#[]` this module wraps, kept under another name.
const STRING_AREF19: &str = "__sabiruby_aref19";

/// That method's native, called directly for the forms 1.8 answers alike.
static STRING_AREF19_NATIVE: std::sync::OnceLock<sabiruby::object::NativeFn> =
    std::sync::OnceLock::new();

pub(super) fn install(vm: &mut Vm) {
    let string = vm.core.string;
    let original = vm.intern("[]");
    let saved = vm.intern(STRING_AREF19);
    vm.alias_method(string, saved, original)
        .expect("String#[] exists");
    if let Some((sabiruby::object::Method::Native(native, _), _)) = vm.find_method(string, saved) {
        let _ = STRING_AREF19_NATIVE.set(native);
    }
    vm.define_method(string, "[]", string_aref);
    // Ruby 1.8 `Object#id`, the old name of `object_id` (`nil.id == 4`).
    // Classes with an id of their own (`RPG::Event#id`) still answer theirs.
    vm.define_method(vm.core.object, "id", object_id18);
    // Ruby 1.8 `Kernel#proc` makes a lambda (`Proc.new` does not): `return`
    // leaves the proc, while its arguments still bind leniently. Essentials
    // registers `Events.onMapChange += proc { |sender, e| return if ... }` and
    // `proc() { ... }` callbacks that are called with two arguments.
    vm.define_module_function(vm.core.kernel, "proc", kernel_proc18)
        .expect("Kernel#proc");
}

fn kernel_proc18(vm: &mut Vm, _: Value, _: &[Value], block: Value) -> Result<Value, VmError> {
    match block {
        Value::Obj(object) => {
            if let ObjKind::Proc(proc_data) = &mut vm.heap.get_mut(object).kind {
                proc_data.strict = true;
                proc_data.loose_arity = true;
                return Ok(block);
            }
            Err(vm.raise_arg("tried to create Proc object without a block"))
        }
        _ => Err(vm.raise_arg("tried to create Proc object without a block")),
    }
}

fn object_id18(_: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(sabiruby::builtins::object::object_id(receiver))
}

/// Ruby 1.8 `String#[]`: a single Integer index answers the byte there as an
/// Integer (`"A"[0] == 65`), or nil past either end. Every other form
/// (ranges, lengths, strings, regexps) answers as in later Rubies.
fn string_aref(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    block: Value,
) -> Result<Value, VmError> {
    if let [Value::Int(index)] = args {
        let bytes = vm.str_bytes(receiver).unwrap_or_default();
        let len = bytes.len() as i64;
        let at = if *index < 0 { len + index } else { *index };
        return Ok(if (0..len).contains(&at) {
            Value::Int(i64::from(bytes[at as usize]))
        } else {
            Value::Nil
        });
    }
    if let Some(native) = STRING_AREF19_NATIVE.get() {
        return native(vm, receiver, args, block);
    }
    let saved = vm.intern(STRING_AREF19);
    vm.funcall(receiver, saved, args, block)
}
