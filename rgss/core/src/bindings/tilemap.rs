//! Host-neutral RGSS1 `Tilemap` compatibility.
//!
//! Essentials uses its own `TilemapRenderer`, but older RGSS1 games create a
//! `Tilemap` directly.  The core keeps the object and its mutable properties
//! alive so those scripts can run in a headless host or hand the data to a
//! renderer later.  Rendering is intentionally owned by the host; this module
//! never creates a GPU resource.

use super::*;

pub(super) fn initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    vm.ivar_set(
        object,
        "@viewport",
        args.first().copied().unwrap_or(Value::Nil),
    );
    for name in ["@tileset", "@map_data", "@flash_data", "@priorities"] {
        vm.ivar_set(object, name, Value::Nil);
    }
    let autotiles = vm.ary_new(vec![Value::Nil; 7]);
    vm.ivar_set(object, "@autotiles", autotiles);
    vm.ivar_set(object, "@ox", Value::Int(0));
    vm.ivar_set(object, "@oy", Value::Int(0));
    vm.ivar_set(object, "@animation", Value::Int(0));
    vm.ivar_set(object, "@visible", Value::True);
    vm.ivar_set(object, "@disposed", Value::False);
    let color_class = vm
        .const_path_get("Color")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Color class is not installed"))?;
    let color = vm.object_new(color_class);
    color_initialize(vm, color, &[], Value::Nil)?;
    let tone_class = vm
        .const_path_get("Tone")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Tone class is not installed"))?;
    let tone = vm.object_new(tone_class);
    tone_initialize(vm, tone, &[], Value::Nil)?;
    vm.ivar_set(object, "@color", color);
    vm.ivar_set(object, "@tone", tone);
    Ok(Value::Nil)
}

pub(super) fn update(
    vm: &mut Vm,
    receiver: Value,
    _: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    if vm.ivar_get(object, "@disposed").truthy() {
        return Ok(Value::Nil);
    }
    let animation = value_i64(vm.ivar_get(object, "@animation"), 0);
    vm.ivar_set(
        object,
        "@animation",
        Value::Int(animation.saturating_add(1)),
    );
    Ok(Value::Nil)
}

pub(super) fn refresh(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Nil)
}

pub(super) fn sync_all(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Nil)
}
