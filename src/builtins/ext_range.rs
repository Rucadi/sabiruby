//! mruby-range-ext (`mrbgems/mruby-range-ext/src/range.c`); its Ruby part is
//! embedded as `src/mrblib/range-ext.mrb`.

use crate::error::VmResult;
use crate::object::Aspec;
use crate::value::Value;
use crate::vm::Vm;

/// `mrb_range_ptr` of a Range (ArgumentError "uninitialized range" for one not filled in), or
/// `None` for a value that is no Range (`mrb_range_p`).
fn parts(vm: &mut Vm, v: Value) -> VmResult<Option<(Value, Value, bool)>> {
    if !super::range::is_range_obj(vm, v) { return Ok(None); }
    super::range::parts(vm, v).map(Some)
}

/// `mrb_cmp`: -1/0/1, or -2 when the two do not compare.
fn cmp(vm: &mut Vm, a: Value, b: Value) -> VmResult<i64> {
    let r: Option<i64> = match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(&y) as i64),
        (Value::Int(x), Value::Float(y)) => (x as f64).partial_cmp(&y).map(|o| o as i64),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(y as f64)).map(|o| o as i64),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(&y).map(|o| o as i64),
        _ => match (vm.str_bytes(a), vm.str_bytes(b)) {
            (Some(p), Some(q)) => Some(p.cmp(q) as i64),
            _ => { let c = vm.intern("<=>"); match vm.funcall(a, c, &[b], Value::Nil)? { Value::Int(i) => Some(i.signum()), _ => None } }
        },
    };
    Ok(r.unwrap_or(-2))
}

/// `r_less`: a < b, or a <= b when not exclusive; false when they do not compare.
fn r_less(vm: &mut Vm, a: Value, b: Value, excl: bool) -> VmResult<bool> {
    Ok(match cmp(vm, a, b)? { -2 | 1 => false, 0 => !excl, _ => true })
}

pub fn init(vm: &mut Vm) {
    let range = vm.core.range;
    vm.define_methods_args(range, &[
        ("cover?", Aspec::req(1), |vm, s, a, _b| {
            let (beg, end, excl) = super::range::parts(vm, s)?;
            let val = a[0];
            if beg.is_nil() && end.is_nil() { return Ok(Value::True); }
            if let Some((beg2, end2, excl2)) = parts(vm, val)? {
                if beg2.is_nil() && end2.is_nil() { return Ok(Value::True); }
                if end.is_nil() {
                    if end2.is_nil() { return Ok(Value::bool(cmp(vm, beg, beg2)? != -2)); }
                    if r_less(vm, end2, beg, excl2)? { return Ok(Value::False); }
                    return Ok(Value::True);
                } else if beg.is_nil() {
                    if beg2.is_nil() { return Ok(Value::bool(cmp(vm, end, end2)? != -2)); }
                    if r_less(vm, end, beg2, excl)? { return Ok(Value::False); }
                    return Ok(Value::True);
                } else {
                    if end2.is_nil() { return Ok(Value::bool(r_less(vm, beg2, end, excl)?)); }
                    if beg2.is_nil() { return Ok(Value::bool(r_less(vm, beg, end2, excl2)?)); }
                    if r_less(vm, end, beg2, excl)? { return Ok(Value::False); }
                    if r_less(vm, end2, beg, excl2)? { return Ok(Value::False); }
                    return Ok(Value::True);
                }
            }
            if beg.is_nil() || r_less(vm, beg, val, false)? {
                if end.is_nil() { return Ok(Value::True); }
                if r_less(vm, val, end, excl)? { return Ok(Value::True); }
            }
            Ok(Value::False)
        }),
        ("size", Aspec::none(), |vm, s, _a, _b| {
            let (beg, end, excl) = super::range::parts(vm, s)?;
            if matches!(beg, Value::Float(_)) { return Err(vm.raise_type("can't iterate from Float")); }
            if beg.is_nil() { return Err(vm.raise_type("can't iterate from nil")); }
            if matches!(beg, Value::Int(_)) && end.is_nil() { return Ok(Value::Float(f64::INFINITY)); }
            let bf = match beg { Value::Int(i) => i as f64, Value::Float(f) => f, _ => return Ok(Value::Nil) };
            let ef = match end { Value::Int(i) => i as f64, Value::Float(f) => f, _ => return Ok(Value::Nil) };
            let mut n = ef - bf;
            let mut err = (libm::fabs(bf) + libm::fabs(ef) + libm::fabs(ef - bf)) * f64::EPSILON;
            if err > 0.5 { err = 0.5; }
            if excl {
                if n <= 0.0 { return Ok(Value::Int(0)); }
                if n < 1.0 { n = 0.0; } else { n = libm::floor(n - err); }
            } else {
                if n < 0.0 { return Ok(Value::Int(0)); }
                n = libm::floor(n + err);
            }
            if (n + 1.0).is_infinite() { return Ok(Value::Float(f64::INFINITY)); }
            Ok(Value::Int(n as i64 + 1))
        }),
        ("__empty_range?", Aspec::req(3), |vm, _s, a, _b| { let (b, e, excl) = (a[0], a[1], a[2].truthy()); if b.is_nil() || e.is_nil() { return Ok(Value::False); } let c = cmp(vm, b, e)?; Ok(Value::bool(c == -2 || c > 0 || (c == 0 && excl))) }),
    ]);
}
