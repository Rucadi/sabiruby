//! Compatibility hooks used by optional rsxp-style Ruby adapters.
//!
//! The desktop rsxp renderer uses these hooks to feed a retained scene graph.
//! SabiRuby's core intentionally snapshots ordinary RGSS objects instead, but
//! exposing the hooks as safe no-ops lets a shared Ruby library load without
//! probing for a particular renderer or creating a second native entry point.

use super::*;

fn empty_array(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(vm.ary_new(Vec::new()))
}

fn false_result(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::False)
}

fn retained_source(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Nil)
}

fn install_kernel(vm: &mut Vm, name: &str, function: sabiruby::object::NativeFn) {
    vm.define_module_function(vm.core.kernel, name, function)
        .expect("RGSS compatibility function");
}

pub(super) fn install(vm: &mut Vm) {
    for name in [
        "__fanplay_install_scalar_setters",
        "__fanplay_touch",
        "__fanplay_watch",
        "__fanplay_unwatch",
        "__fanplay_forget",
        "__fanplay_volatile",
        "__fanplay_scroll_tiles",
        "__fanplay_scroll_rgss_tiles",
        "__fanplay_world_camera",
        "__fanplay_world_release",
        "__fanplay_world_metadata_changed",
        "__fanplay_world_unbind",
        "__fanplay_sprite_sync",
    ] {
        install_kernel(vm, name, noop);
    }
    install_kernel(vm, "__fanplay_apply_rgss_tiles", false_result);
    install_kernel(vm, "__fanplay_dirty_resources", empty_array);
    install_kernel(vm, "__fanplay_take_render_commands", empty_array);
    install_kernel(vm, "__fanplay_native_bindings_loaded", noop);
    install_kernel(vm, "__fanplay_retained_source", retained_source);
}
