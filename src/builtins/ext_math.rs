//! mruby-math (`mrbgems/mruby-math/src/math.c`): the `Math` module over `libm`, with the
//! reference's domain checks (`Math::DomainError`). No Ruby part.

use alloc::{format, vec};

use crate::error::VmResult;
use crate::object::{Aspec, ClassData, ObjKind};
use crate::value::{Slot, Value};
use crate::vm::Vm;

/// `mrb_as_float` (`mrb_ensure_float_type`, src/object.c): an Integer of either width, a
/// Float, a Rational and a Complex with no imaginary part (`mrb_complex_to_f`) become a Float.
/// The message for anything else is still SabiRuby's (rc2's "%Y cannot be converted to Float"
/// is `conv-float-msg`, plan Y4).
pub(crate) fn as_float(vm: &mut Vm, v: Value) -> VmResult<f64> {
    if let Some(f) = super::numeric::num_f64(vm, v) { return Ok(f); }
    if super::ext_complex::is_complex(vm, v) { return super::ext_complex::to_f64(vm, v); }
    match v {
        Value::Nil => Err(vm.raise_type("can't convert nil into Float")),
        _ => { let d = vm.describe_for_type_error(v); Err(vm.raise_type(&format!("can't convert {d} into Float"))) }
    }
}

fn domain_error(vm: &mut Vm, func: &str) -> crate::error::VmError {
    let math = vm.intern("Math");
    let de = vm.intern("DomainError");
    let cls = vm.const_get(vm.core.object, math).and_then(|m| m.obj()).and_then(|m| vm.const_get(m, de)).and_then(|c| c.obj()).unwrap_or(vm.core.standard_error);
    vm.raise(cls, &format!("Numerical argument is out of domain - {func}"))
}

macro_rules! unary {
    ($name:ident, $f:expr) => {
        fn $name(vm: &mut Vm, _s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
            let x = as_float(vm, a[0])?;
            Ok(Value::Float($f(x)))
        }
    };
    ($name:ident, $f:expr, $check:expr, $what:literal) => {
        fn $name(vm: &mut Vm, _s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
            let x = as_float(vm, a[0])?;
            if !($check)(x) { return Err(domain_error(vm, $what)); }
            Ok(Value::Float($f(x)))
        }
    };
}

// the domain checks are the reference's comparisons written as what passes (`x < -1.0 || x >
// 1.0` is the error in `math_asin`): a NaN fails every comparison, so it passes and answers NaN
unary!(math_sin, libm::sin);
unary!(math_cos, libm::cos);
unary!(math_tan, libm::tan);
unary!(math_asin, libm::asin, |x: f64| !(x < -1.0 || x > 1.0), "asin");
unary!(math_acos, libm::acos, |x: f64| !(x < -1.0 || x > 1.0), "acos");
unary!(math_atan, libm::atan);
unary!(math_sinh, libm::sinh);
unary!(math_cosh, libm::cosh);
unary!(math_tanh, libm::tanh);
unary!(math_asinh, libm::asinh);
unary!(math_acosh, libm::acosh, |x: f64| !(x < 1.0), "acosh");
unary!(math_atanh, libm::atanh, |x: f64| !(x < -1.0 || x > 1.0), "atanh");
unary!(math_exp, libm::exp);
unary!(math_expm1, libm::expm1);
unary!(math_log1p, libm::log1p, |x: f64| !(x < -1.0), "log1p");
unary!(math_log2, libm::log2, |x: f64| !(x < 0.0), "log2");
unary!(math_log10, libm::log10, |x: f64| !(x < 0.0), "log10");
unary!(math_sqrt, libm::sqrt, |x: f64| !(x < 0.0), "sqrt");
unary!(math_cbrt, libm::cbrt);
unary!(math_erf, libm::erf);
unary!(math_erfc, libm::erfc);

fn math_atan2(vm: &mut Vm, _s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let (y, x) = (as_float(vm, a[0])?, as_float(vm, a[1])?);
    Ok(Value::Float(libm::atan2(y, x)))
}

fn math_hypot(vm: &mut Vm, _s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let (x, y) = (as_float(vm, a[0])?, as_float(vm, a[1])?);
    Ok(Value::Float(libm::hypot(x, y)))
}

/// `log(x)`, `log(x, base)`
fn math_log(vm: &mut Vm, _s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let x = as_float(vm, a[0])?;
    if x < 0.0 { return Err(domain_error(vm, "log")); }
    let mut r = libm::log(x);
    if a.len() == 2 {
        let base = as_float(vm, a[1])?;
        if base < 0.0 { return Err(domain_error(vm, "log")); }
        r /= libm::log(base);
    }
    Ok(Value::Float(r))
}

fn math_frexp(vm: &mut Vm, _s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let x = as_float(vm, a[0])?;
    let (m, e) = libm::frexp(x);
    Ok(vm.ary_new(vec![Value::Float(m), Value::Int(e as i64)]))
}

fn math_ldexp(vm: &mut Vm, _s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let x = as_float(vm, a[0])?;
    let i = vm.expect_int(a[1], "exponent")?;
    Ok(Value::Float(libm::ldexp(x, i as i32)))
}

pub fn init(vm: &mut Vm) {
    let math = vm.define_module("Math");
    // `Math::DomainError < StandardError`
    let n = vm.intern("DomainError");
    let de = vm.heap.alloc(vm.core.class, ObjKind::Class(ClassData { name: Some(n), superclass: Some(vm.core.standard_error), outer: Some(math), ..Default::default() }));
    vm.singleton_class(Value::Obj(de)).expect("metaclass");
    vm.heap.class_mut(math).consts.insert(n, Slot::from(Value::Obj(de)));
    for (name, v) in [("PI", core::f64::consts::PI), ("E", core::f64::consts::E)] {
        let n = vm.intern(name);
        vm.heap.class_mut(math).consts.insert(n, Slot::from(Value::Float(v)));
    }
    let fns: &[(&str, Aspec, crate::object::NativeFn)] = &[
        ("sin", Aspec::req(1), math_sin), ("cos", Aspec::req(1), math_cos), ("tan", Aspec::req(1), math_tan),
        ("asin", Aspec::req(1), math_asin), ("acos", Aspec::req(1), math_acos), ("atan", Aspec::req(1), math_atan), ("atan2", Aspec::req(2), math_atan2),
        ("sinh", Aspec::req(1), math_sinh), ("cosh", Aspec::req(1), math_cosh), ("tanh", Aspec::req(1), math_tanh),
        ("asinh", Aspec::req(1), math_asinh), ("acosh", Aspec::req(1), math_acosh), ("atanh", Aspec::req(1), math_atanh),
        ("exp", Aspec::req(1), math_exp), ("expm1", Aspec::req(1), math_expm1), ("log1p", Aspec::req(1), math_log1p), ("log", Aspec::req(1) | Aspec::opt(1), math_log), ("log2", Aspec::req(1), math_log2), ("log10", Aspec::req(1), math_log10),
        ("sqrt", Aspec::req(1), math_sqrt), ("cbrt", Aspec::req(1), math_cbrt), ("frexp", Aspec::req(1), math_frexp), ("ldexp", Aspec::req(2), math_ldexp), ("hypot", Aspec::req(2), math_hypot),
        ("erf", Aspec::req(1), math_erf), ("erfc", Aspec::req(1), math_erfc),
    ];
    // module functions: public on `Math` itself, private as instance methods
    let msc = vm.singleton_class(Value::Obj(math)).expect("Math singleton");
    vm.define_methods_args(msc, fns);
    vm.define_methods_args(math, fns);
    for (name, _, _) in fns {
        let n = vm.intern(name);
        let _ = vm.set_visibility(math, n, crate::object::Vis::Private);
    }
}
