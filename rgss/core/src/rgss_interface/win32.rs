//! Portable `Win32API` compatibility.
//!
//! RGSS games frequently construct `Win32API` objects even when the function
//! is only used by an optional Windows integration.  The desktop rsxp binding
//! validates the legacy signature and then delegates to a platform shim.  The
//! SabiRuby core does the same validation and provides deterministic host
//! fallbacks for the window/input queries that can be represented without an
//! OS ABI.  It deliberately does not load arbitrary DLLs: doing that would tie
//! the embeddable core to one process ABI and defeat its WASM/other-host use.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Void,
    Integer,
    Pointer,
}

fn parse_kind(code: u8) -> Option<Kind> {
    match code {
        b'0' | b'v' | b'V' => Some(Kind::Void),
        b'i' | b'I' | b'l' | b'L' | b'n' | b'N' => Some(Kind::Integer),
        b'p' | b'P' | b's' | b'S' => Some(Kind::Pointer),
        _ => None,
    }
}

fn signature(vm: &mut Vm, value: Value, name: &str) -> Result<Vec<u8>, VmError> {
    if let Some(bytes) = vm.str_bytes(value) {
        return Ok(bytes.to_vec());
    }
    if let Some(items) = vm.ary_vals(value) {
        let mut output = Vec::new();
        for item in items {
            let bytes = vm.expect_str(item, name)?;
            output.extend(bytes);
        }
        return Ok(output);
    }
    Err(vm.raise_type("Win32API signature must be a String or Array"))
}

/// Ruby's `Win32API.c` (and mkxp-z's MiniFFI after it) picks the type letters
/// out of the import string and skips every other character, so scripts that
/// pass `'%w(l p)'` or `%(l, l, i)` by mistake still declare `l p` and `l l i`.
fn validate_inputs(vm: &mut Vm, bytes: &[u8]) -> Result<usize, VmError> {
    let mut count = 0;
    let mut has_non_void = false;
    let mut has_void = false;
    for code in bytes.iter().copied() {
        let Some(kind) = parse_kind(code) else {
            continue;
        };
        match kind {
            Kind::Void => has_void = true,
            Kind::Integer | Kind::Pointer => {
                has_non_void = true;
                count += 1;
            }
        }
    }
    if has_void && has_non_void {
        return Err(vm.raise_arg("void is only valid for an empty Win32API argument list"));
    }
    if count > 16 {
        return Err(vm.raise_arg("Win32API supports at most 16 arguments"));
    }
    Ok(count)
}

fn normalized_name(vm: &Vm, value: Value) -> String {
    vm.str_bytes(value)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

pub(super) fn install(vm: &mut Vm) {
    let class = install_class(vm, "Win32API");
    vm.define_method(class, "initialize", initialize);
    vm.define_method(class, "call", call);
    vm.define_method(class, "Call", call);
}

fn initialize(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    if !(3..=5).contains(&args.len()) {
        return Err(vm.raise_arg(
            "expected DLL, function, argument types, optional return type and calling convention",
        ));
    }
    let object = object_instance(vm, receiver)?;
    let dll = normalized_name(vm, args[0]);
    let function = normalized_name(vm, args[1]);
    let input = signature(vm, args[2], "argument types")?;
    let input_count = validate_inputs(vm, &input)?;
    // The export type is read the same way: its first type letter, void if none.
    let output = if let Some(value) = args.get(3) {
        signature(vm, *value, "return type")?
    } else {
        b"0".to_vec()
    };
    let output = output
        .iter()
        .copied()
        .find(|code| parse_kind(*code).is_some())
        .map_or_else(|| b"0".to_vec(), |code| vec![code]);
    if let Some(convention) = args.get(4) {
        let convention = normalized_name(vm, *convention);
        if convention != "stdcall" && convention != "cdecl" {
            return Err(vm.raise_arg(&format!(
                "unsupported Win32API calling convention: {convention}"
            )));
        }
    }
    let dll_value = vm.str_new(dll.as_bytes());
    let function_value = vm.str_new(function.as_bytes());
    let output_value = vm.str_new(&output);
    vm.ivar_set(object, "@__sabiruby_win32_dll", dll_value);
    vm.ivar_set(object, "@__sabiruby_win32_function", function_value);
    vm.ivar_set(
        object,
        "@__sabiruby_win32_input_count",
        Value::Int(input_count as i64),
    );
    vm.ivar_set(object, "@__sabiruby_win32_output", output_value);
    Ok(Value::Nil)
}

fn call(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let expected = value_i64(vm.ivar_get(object, "@__sabiruby_win32_input_count"), 0) as usize;
    if args.len() != expected {
        return Err(vm.argnum_error(args.len(), &expected.to_string()));
    }
    let dll = normalized_name(vm, vm.ivar_get(object, "@__sabiruby_win32_dll"));
    let function = normalized_name(vm, vm.ivar_get(object, "@__sabiruby_win32_function"));
    let dll_lower = dll.to_ascii_lowercase();
    let function_lower = function.to_ascii_lowercase();
    let state = state_mut(vm);

    // Window handles and geometry are host concepts.  A stable non-zero
    // handle keeps legacy scripts from treating an embedded window as absent.
    if matches!(
        function_lower.as_str(),
        "getforegroundwindow" | "findwindow" | "findwindowa" | "findwindowex" | "findwindowexa"
    ) {
        return Ok(Value::Int(42));
    }
    if function_lower == "getsystemmetrics" {
        return Ok(match args.first().map(|value| value_i64(*value, -1)) {
            Some(0) => Value::Int(i64::from(state.width)),
            Some(1) => Value::Int(i64::from(state.height)),
            _ => Value::Int(0),
        });
    }
    if function_lower == "getclientrect" || function_lower == "getwindowrect" {
        return Ok(Value::Int(1));
    }
    if function_lower == "getkeystate" || function_lower == "getasynckeystate" {
        return Ok(Value::Int(0));
    }
    if function_lower == "getcurrentprocessid" || function_lower == "getcurrentthreadid" {
        return Ok(Value::Int(0));
    }
    if function_lower == "showcursor"
        || function_lower == "showwindow"
        || function_lower == "setwindowpos"
        || function_lower == "setwindowtext"
        || function_lower == "setwindowtexta"
        || function_lower == "setwindowlong"
        || function_lower == "setwindowlonga"
    {
        return Ok(Value::Int(1));
    }
    if dll_lower.contains("kgl2") {
        // KGL2 is an optional graphics extension used by older projects.  The
        // normal RGSS bitmap path remains authoritative in SabiRuby, so report
        // success for its probes without fabricating an OS framebuffer.
        return Ok(Value::Int(if function_lower == "kglversion" {
            200
        } else {
            1
        }));
    }

    // Unknown calls are intentionally tolerant like rsxp's portable shim. A
    // game can keep optional Windows integrations enabled while the embedding
    // host supplies a richer implementation later.
    Ok(Value::Int(0))
}
