//! Ruby compatibility installed directly through [`sabiruby::Vm`].
//!
//! This is deliberately kept separate from the RGSS service bindings.  It
//! supplies the small Ruby object model that Essentials expects before its
//! scripts start (Encoding, RPG data objects, Table methods, and a few
//! portable stdlib shims).  None of these methods create a window, access the
//! host filesystem, or depend on WGPU.

use super::*;
use sabiruby::object::NativeFn;

fn class_function(vm: &mut Vm, class: ObjId, name: &str, function: NativeFn) {
    let singleton = vm
        .singleton_class(Value::Obj(class))
        .expect("RGSS compatibility singleton");
    vm.define_method(singleton, name, function);
}

fn define_module_function(vm: &mut Vm, module: ObjId, name: &str, function: NativeFn) {
    vm.define_module_function(module, name, function)
        .expect("RGSS compatibility module function");
}

fn object_id(vm: &mut Vm, receiver: Value, message: &str) -> Result<ObjId, VmError> {
    receiver.obj().ok_or_else(|| vm.raise_type(message))
}

fn string_arg(vm: &mut Vm, value: Value, message: &str) -> Result<String, VmError> {
    let bytes = vm.expect_str(value, message)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ---------------------------------------------------------------------------
// Encoding

fn encoding_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "Encoding receiver is not an object")?;
    let name = args.first().copied().unwrap_or(Value::Nil);
    vm.ivar_set(object, "@name", name);
    Ok(Value::Nil)
}

fn encoding_name(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "Encoding receiver is not an object")?;
    Ok(vm.ivar_get(object, "@name"))
}

fn encoding_to_s(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    block: Value,
) -> Result<Value, VmError> {
    encoding_name(vm, receiver, args, block)
}

fn encoding_equal(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "Encoding receiver is not an object")?;
    let Some(other) = args.first().copied() else {
        return Ok(Value::False);
    };
    let name = vm.ivar_get(object, "@name");
    Ok(Value::bool(other == receiver || other == name))
}

fn encoding_hash(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "Encoding receiver is not an object")?;
    let name = vm.ivar_get(object, "@name");
    vm.funcall(name, vm.s.hash, &[], Value::Nil)
}

fn encoding_find(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let module = object_id(vm, receiver, "Encoding receiver is not a module")?;
    let value = args.first().copied().unwrap_or(Value::Nil);
    let utf8_name = vm.intern("UTF_8");
    let utf8 = vm.const_get(module, utf8_name).unwrap_or(Value::Nil);
    if value == utf8
        || vm
            .str_bytes(value)
            .is_some_and(|bytes| bytes.eq_ignore_ascii_case(b"UTF-8"))
    {
        return Ok(utf8);
    }
    Err(vm.raise_arg(&format!("unknown encoding: {value:?}")))
}

fn encoding_default_internal(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let module = object_id(vm, receiver, "Encoding receiver is not a module")?;
    if let Some(value) = args.first().copied() {
        vm.ivar_set(module, "@default_internal", value);
        Ok(value)
    } else {
        Ok(vm.ivar_get(module, "@default_internal"))
    }
}

fn encoding_default_external(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let module = object_id(vm, receiver, "Encoding receiver is not a module")?;
    if let Some(value) = args.first().copied() {
        vm.ivar_set(module, "@default_external", value);
        Ok(value)
    } else {
        Ok(vm.ivar_get(module, "@default_external"))
    }
}

fn install_encoding(vm: &mut Vm) -> Result<(), VmError> {
    let encoding = vm.define_module("Encoding");
    let class = vm.define_class_under(encoding, "RGSSEncoding", vm.core.object);
    vm.define_method(class, "initialize", encoding_initialize);
    vm.define_method(class, "name", encoding_name);
    vm.define_method(class, "to_s", encoding_to_s);
    vm.define_method(class, "==", encoding_equal);
    vm.define_method(class, "eql?", encoding_equal);
    vm.define_method(class, "hash", encoding_hash);
    let utf8 = vm.object_new(class);
    let utf8_name = vm.str_new(b"UTF-8");
    encoding_initialize(vm, utf8, &[utf8_name], Value::Nil)?;
    vm.const_set(encoding, "UTF_8", utf8)?;
    vm.ivar_set(encoding, "@default_internal", utf8);
    vm.ivar_set(encoding, "@default_external", utf8);
    define_module_function(vm, encoding, "find", encoding_find);
    define_module_function(vm, encoding, "default_internal", encoding_default_internal);
    define_module_function(vm, encoding, "default_internal=", encoding_default_internal);
    define_module_function(vm, encoding, "default_external", encoding_default_external);
    define_module_function(vm, encoding, "default_external=", encoding_default_external);
    Ok(())
}

// ---------------------------------------------------------------------------
// Generic compatibility classes and constants

fn install_error_classes(vm: &mut Vm) -> Result<(), VmError> {
    let system_exit = vm.define_class("SystemExit", vm.core.exception);
    let errno = vm.define_module("Errno");
    for name in ["ENOENT", "EINVAL", "EACCES", "EISDIR"] {
        let class = vm.define_class_under(errno, name, vm.core.standard_error);
        vm.const_set(errno, name, Value::Obj(class))?;
    }
    vm.const_set(vm.core.object, "SystemExit", Value::Obj(system_exit))
}

fn input_constants(vm: &mut Vm) -> Result<(), VmError> {
    let input = vm
        .const_path_get("Input")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Input module is not installed"))?;
    for (name, value) in [
        ("DOWN", 2),
        ("LEFT", 4),
        ("RIGHT", 6),
        ("UP", 8),
        ("A", 11),
        ("B", 12),
        ("C", 13),
        ("X", 14),
        ("Y", 15),
        ("Z", 16),
        ("L", 17),
        ("R", 18),
        ("SHIFT", 21),
        ("CTRL", 22),
        ("ALT", 23),
        ("F5", 25),
        ("F6", 26),
        ("F7", 27),
        ("F8", 28),
        ("F9", 29),
        ("MOUSELEFT", 1001),
        ("MOUSEMIDDLE", 1002),
        ("MOUSERIGHT", 1003),
        ("BACK", 12),
        ("SPECIAL", 16),
        ("ACTION", 11),
        ("USE", 13),
        ("JUMPUP", 14),
        ("JUMPDOWN", 15),
        ("AUX1", 17),
        ("AUX2", 18),
    ] {
        vm.const_set(input, name, Value::Int(value))?;
    }
    Ok(())
}

fn font_default(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    name: &str,
) -> Result<Value, VmError> {
    let class = object_id(vm, receiver, "Font receiver is not a class")?;
    if let Some(value) = args.first().copied() {
        vm.ivar_set(class, name, value);
        Ok(value)
    } else {
        Ok(vm.ivar_get(class, name))
    }
}

macro_rules! font_default_method {
    ($name:ident, $ivar:literal) => {
        fn $name(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
            font_default(vm, receiver, args, $ivar)
        }
    };
}

font_default_method!(font_default_name, "@default_name");
font_default_method!(font_default_size, "@default_size");
font_default_method!(font_default_bold, "@default_bold");
font_default_method!(font_default_italic, "@default_italic");
font_default_method!(font_default_outline, "@default_outline");
font_default_method!(font_default_shadow, "@default_shadow");
font_default_method!(font_default_out_color, "@default_out_color");

fn install_font_defaults(vm: &mut Vm) -> Result<(), VmError> {
    let font = vm
        .const_path_get("Font")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Font class is not installed"))?;
    let color = new_color(vm, 0, 0, 0, 128)?;
    for (name, value) in [
        ("@default_name", vm.str_new(b"Arial")),
        ("@default_size", Value::Int(22)),
        ("@default_bold", Value::False),
        ("@default_italic", Value::False),
        ("@default_outline", Value::False),
        ("@default_shadow", Value::False),
        ("@default_out_color", color),
    ] {
        vm.ivar_set(font, name, value);
    }
    for (name, function) in [
        ("default_name", font_default_name as NativeFn),
        ("default_name=", font_default_name),
        ("default_size", font_default_size),
        ("default_size=", font_default_size),
        ("default_bold", font_default_bold),
        ("default_bold=", font_default_bold),
        ("default_italic", font_default_italic),
        ("default_italic=", font_default_italic),
        ("default_outline", font_default_outline),
        ("default_outline=", font_default_outline),
        ("default_shadow", font_default_shadow),
        ("default_shadow=", font_default_shadow),
        ("default_out_color", font_default_out_color),
        ("default_out_color=", font_default_out_color),
    ] {
        class_function(vm, font, name, function);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// RGSS data objects and Table's Ruby-facing methods

fn rgss_data_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "RGSSData receiver is not an object")?;
    let values = args.first().copied().unwrap_or_else(|| vm.hash_new());
    vm.ivar_set(object, "@values", values);
    Ok(Value::Nil)
}

fn rgss_data_method_missing(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "RGSSData receiver is not an object")?;
    let Some(Value::Sym(name)) = args.first().copied() else {
        return Ok(Value::Nil);
    };
    let name = vm.sym_name(name);
    let value = vm.ivar_get(object, &format!("@{name}"));
    if !value.is_nil() {
        return Ok(value);
    }
    let values = vm.ivar_get(object, "@values");
    let symbol = vm.intern(&name);
    vm.hash_get(values, Value::Sym(symbol))
        .map(|value| value.unwrap_or(Value::Nil))
}

fn table_dimensions_value(
    vm: &mut Vm,
    receiver: Value,
    _: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "Table receiver is not an object")?;
    Ok(vm.ivar_get(object, "@dimensions"))
}

fn table_size(vm: &mut Vm, receiver: Value, name: &str) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "Table receiver is not an object")?;
    Ok(vm.ivar_get(object, name))
}

macro_rules! table_size_method {
    ($name:ident, $ivar:literal) => {
        fn $name(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
            table_size(vm, receiver, $ivar)
        }
    };
}

table_size_method!(table_xsize, "@xsize");
table_size_method!(table_ysize, "@ysize");
table_size_method!(table_zsize, "@zsize");

fn table_aref(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(match table_get(vm, receiver, args, Value::Nil)? {
        Value::Nil => Value::Int(0),
        value => value,
    })
}

fn table_aset(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    if !(2..=4).contains(&args.len()) {
        return Err(vm.raise_arg("expected one to three coordinates and a value"));
    }
    let mut coordinates = [Value::Int(0); 4];
    let dimensions = args.len() - 1;
    coordinates[..dimensions].copy_from_slice(&args[..dimensions]);
    coordinates[3] = args[dimensions];
    table_set(vm, receiver, &coordinates, Value::Nil)
}

fn table_clone(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let source = object_id(vm, receiver, "Table receiver is not an object")?;
    let copy = vm.object_new(vm.real_class_of(receiver));
    let target = object_id(vm, copy, "Table clone is not an object")?;
    for name in [
        "@dimensions",
        "@xsize",
        "@ysize",
        "@zsize",
        "@data",
        "@__sabiruby_revision",
    ] {
        vm.ivar_set(target, name, vm.ivar_get(source, name));
    }
    if let Some(values) = vm.ary_vals(vm.ivar_get(source, "@data")) {
        let data = vm.ary_new(values);
        vm.ivar_set(target, "@data", data);
    }
    Ok(copy)
}

// ---------------------------------------------------------------------------
// RPG::AudioFile and data loading

fn audio_file_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "AudioFile receiver is not an object")?;
    let name = args.first().copied().unwrap_or_else(|| vm.str_new(b""));
    vm.ivar_set(object, "@name", name);
    vm.ivar_set(
        object,
        "@volume",
        args.get(1).copied().unwrap_or(Value::Int(100)),
    );
    vm.ivar_set(
        object,
        "@pitch",
        args.get(2).copied().unwrap_or(Value::Int(100)),
    );
    Ok(Value::Nil)
}

fn data_value(vm: &mut Vm, receiver: Value, name: &str, args: &[Value]) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "AudioFile receiver is not an object")?;
    if let Some(value) = args.first().copied() {
        vm.ivar_set(object, name, value);
        Ok(value)
    } else {
        Ok(vm.ivar_get(object, name))
    }
}

macro_rules! audio_value_method {
    ($name:ident, $ivar:literal) => {
        fn $name(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
            data_value(vm, receiver, $ivar, args)
        }
    };
}

audio_value_method!(audio_file_name, "@name");
audio_value_method!(audio_file_volume, "@volume");
audio_value_method!(audio_file_pitch, "@pitch");

fn save_data_stub(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Nil)
}

fn load_data_compat(vm: &mut Vm, _: Value, args: &[Value], block: Value) -> Result<Value, VmError> {
    let path = string_arg(vm, args.first().copied().unwrap_or(Value::Nil), "data path")?;
    if path.contains("PluginScripts") {
        return rgss_plugin_scripts(vm, Value::Nil, &[], block);
    }
    rgss_load_data(vm, Value::Nil, args, block)
}

// ---------------------------------------------------------------------------
// Zlib/JSON/Kernel/Thread/String shims

fn identity_argument(_: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(args.first().copied().unwrap_or(Value::Nil))
}

fn json_parse(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(vm.hash_new())
}

fn kernel_require(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    block: Value,
) -> Result<Value, VmError> {
    let name = string_arg(
        vm,
        args.first().copied().unwrap_or(Value::Nil),
        "feature name",
    )?;
    if name == "socket" || name == "io/wait" {
        return Ok(Value::True);
    }
    let require_name = vm.intern("__sabiruby_require");
    vm.funcall(receiver, require_name, args, block)
}

fn kernel_exit(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let class = vm
        .const_path_get("SystemExit")
        .and_then(Value::obj)
        .unwrap_or(vm.core.exception);
    Err(vm.raise(class, ""))
}

fn thread_new(vm: &mut Vm, _: Value, _: &[Value], block: Value) -> Result<Value, VmError> {
    if !block.is_nil() {
        vm.call_block(block, &[])?;
    }
    let class = vm
        .const_path_get("RGSSThreadStub")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("RGSSThreadStub is not installed"))?;
    Ok(vm.object_new(class))
}

fn thread_current(_: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(receiver)
}

fn string_force_encoding(
    _: &mut Vm,
    receiver: Value,
    _: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    Ok(receiver)
}

fn string_encoding(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(vm.const_path_get("Encoding::UTF_8").unwrap_or(Value::Nil))
}

fn string_sub_bang(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    block: Value,
) -> Result<Value, VmError> {
    if args.first().is_some_and(|value| value.is_nil()) {
        return Ok(receiver);
    }
    let sub_name = vm.intern("__sabiruby_sub_bang");
    vm.funcall(receiver, sub_name, args, block)
}

fn dynamic_method_missing(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "dynamic property receiver is not an object")?;
    let Some(Value::Sym(symbol)) = args.first().copied() else {
        return Err(vm.raise_arg("dynamic property name is required"));
    };
    let mut name = vm.sym_name(symbol);
    if name.ends_with('=') {
        name.pop();
        let value = args.get(1).copied().unwrap_or(Value::Nil);
        vm.ivar_set(object, &format!("@{name}"), value);
        return Ok(value);
    }
    let query = name.ends_with('?');
    if query {
        name.pop();
    }
    let value = vm.ivar_get(object, &format!("@{name}"));
    if query {
        Ok(Value::bool(!value.is_nil() && value != Value::False))
    } else {
        Ok(value)
    }
}

fn cache_clear(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_id(vm, receiver, "cache receiver is not a module")?;
    let cache = vm.hash_new();
    vm.ivar_set(object, "@cache", cache);
    // Sprites and callers can still own cached bitmaps; clearing a lookup
    // table must not dispose objects that remain in use.
    Ok(Value::Nil)
}

fn install_data_classes(vm: &mut Vm) -> Result<(), VmError> {
    let data = vm.define_class("RGSSData", vm.core.object);
    vm.define_method(data, "initialize", rgss_data_initialize);
    vm.define_closure(data, "method_missing", rgss_data_method_missing);
    let rpg = vm.define_module("RPG");
    let cache = vm.define_module_under(rpg, "Cache");
    let cache_value = vm.hash_new();
    vm.ivar_set(cache, "@cache", cache_value);
    define_module_function(vm, cache, "clear", cache_clear);
    let audio = vm.define_class_under(rpg, "AudioFile", data);
    vm.define_method(audio, "initialize", audio_file_initialize);
    for (name, function) in [
        ("name", audio_file_name as NativeFn),
        ("name=", audio_file_name),
        ("volume", audio_file_volume),
        ("volume=", audio_file_volume),
        ("pitch", audio_file_pitch),
        ("pitch=", audio_file_pitch),
    ] {
        vm.define_method(audio, name, function);
    }
    Ok(())
}

fn install_table_methods(vm: &mut Vm) -> Result<(), VmError> {
    let table = vm
        .const_path_get("Table")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Table class is not installed"))?;
    for (name, function) in [
        ("dimensions", table_dimensions_value as NativeFn),
        ("xsize", table_xsize),
        ("ysize", table_ysize),
        ("zsize", table_zsize),
        ("[]=", table_aset),
        ("[]", table_aref),
        ("resize", table_resize),
        ("clone", table_clone),
    ] {
        vm.define_method(table, name, function);
    }
    Ok(())
}

pub(super) fn install(vm: &mut Vm) -> Result<(), VmError> {
    install_encoding(vm)?;
    install_error_classes(vm)?;
    input_constants(vm)?;
    install_font_defaults(vm)?;
    install_data_classes(vm)?;
    install_table_methods(vm)?;

    // `Graphics.play_movie` is the public RGSS entry point.  The service
    // bindings keep the decoder operation private (`__sabiruby_play_movie`)
    // so hosts can choose how to present frames; expose the same operation
    // through the VM without compiling a Ruby shim.
    if let Some(graphics) = vm.const_path_get("Graphics").and_then(Value::obj) {
        define_module_function(vm, graphics, "play_movie", graphics_play_movie);
    }

    if let Some(system) = vm.const_path_get("System").and_then(Value::obj) {
        let version = vm.str_new(b"2.4.2/d13f35c");
        vm.const_set(system, "VERSION", version)?;
    }
    let load_path = vm.ary_new(Vec::new());
    vm.global_set("$:", load_path);
    vm.global_set("$LOAD_PATH", load_path);
    if vm.const_path_get("TOPLEVEL_BINDING").is_none() {
        let top_level_binding = vm.top_level_binding();
        let object = vm.core.object;
        vm.const_set(object, "TOPLEVEL_BINDING", top_level_binding)?;
    }

    let zlib = vm.define_module("Zlib");
    let inflate = vm.define_module_under(zlib, "Inflate");
    let deflate = vm.define_module_under(zlib, "Deflate");
    define_module_function(vm, inflate, "inflate", identity_argument);
    define_module_function(vm, deflate, "deflate", identity_argument);

    let http = vm.define_module("HTTPLite");
    let json = vm.define_module_under(http, "JSON");
    let parser_error = vm.define_class_under(json, "ParserError", vm.core.standard_error);
    vm.const_set(json, "ParserError", Value::Obj(parser_error))?;
    define_module_function(vm, json, "parse", json_parse);

    let require_alias = vm.intern("__sabiruby_require");
    let require_name = vm.intern("require");
    vm.alias_method(vm.core.kernel, require_alias, require_name)?;
    define_module_function(vm, vm.core.kernel, "require", kernel_require);
    define_module_function(vm, vm.core.kernel, "exit!", kernel_exit);
    define_module_function(vm, vm.core.kernel, "load_data", load_data_compat);
    define_module_function(vm, vm.core.kernel, "save_data", save_data_stub);

    let stub = vm.define_class("RGSSThreadStub", vm.core.object);
    vm.define_method(stub, "status", noop);
    let thread = vm.define_class("Thread", vm.core.object);
    class_function(vm, thread, "new", thread_new);
    class_function(vm, thread, "current", thread_current);
    class_function(vm, thread, "exit", noop);

    let sub_alias = vm.intern("__sabiruby_sub_bang");
    let sub_name = vm.intern("sub!");
    vm.alias_method(vm.core.string, sub_alias, sub_name)?;
    vm.define_method(vm.core.string, "force_encoding", string_force_encoding);
    vm.define_method(vm.core.string, "encoding", string_encoding);
    vm.define_method(vm.core.string, "sub!", string_sub_bang);

    let dynamic = vm.define_module("RGSSDynamicProperties");
    vm.define_closure(dynamic, "method_missing", dynamic_method_missing);
    for name in [
        "Bitmap", "Sprite", "Viewport", "Plane", "Window", "Color", "Tone", "Rect", "Table",
        "Font", "Tilemap",
    ] {
        if let Some(Value::Obj(class)) = vm.const_path_get(name) {
            vm.include_module(class, dynamic);
        }
    }
    Ok(())
}
