//! Integer and Float.
//!
//! An Integer is a `Value::Int` while it fits in an `i64` and a heap
//! [`BigInt`](crate::bigint::BigInt) when it does not; every operation that can leave the
//! `i64` range promotes instead of raising, as mruby's `#ifdef MRB_USE_BIGINT` branches in
//! `src/numeric.c` do. A result that fits is always normalized back to `Value::Int`
//! (`Vm::bint_value`), so the two representations never hold the same number.

use alloc::{format, string::String, string::ToString, vec};

use crate::object::Aspec;
use crate::argc;
use crate::bigint::BigInt;
use crate::error::{VmError, VmResult};
use crate::value::{Slot, Value};
use crate::vm::Vm;

/// Ruby's floor division.
pub fn div_floor(p: i64, q: i64) -> i64 {
    let d = p.wrapping_div(q);
    if (p.wrapping_rem(q) != 0) && ((p < 0) != (q < 0)) { d - 1 } else { d }
}
pub fn mod_floor(p: i64, q: i64) -> i64 {
    let m = p.wrapping_rem(q);
    if m != 0 && ((m < 0) != (q < 0)) { m + q } else { m }
}

/// Float formatting close to mruby's `flo_to_s` (`%.16g` with a forced `.0`).
pub fn float_to_s(f: f64) -> String {
    if f.is_nan() { return "NaN".into(); }
    if f.is_infinite() { return if f > 0.0 { "Infinity".into() } else { "-Infinity".into() }; }
    if f == 0.0 { return if f.is_sign_negative() { "-0.0".into() } else { "0.0".into() }; }
    let abs = f.abs();
    if !(1e-4..1e15).contains(&abs) {
        // exponent form: d.ddde+XX
        let s = format!("{:e}", f);
        let (mant, exp) = s.split_once('e').unwrap();
        let mant = if mant.contains('.') { mant.to_string() } else { format!("{mant}.0") };
        let exp: i32 = exp.parse().unwrap();
        return format!("{mant}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let s = format!("{}", f);
    if s.contains('.') { s } else { format!("{s}.0") }
}

pub(crate) fn num_args(vm: &mut Vm, s: Value, a: &[Value]) -> VmResult<(Value, Value)> {
    argc!(vm, a, 1);
    Ok((s, a[0]))
}

/// `mrb_as_float` (`mrb_ensure_float_type`, src/object.c of mruby 4.1.0-rc2): an Integer of
/// either width, a Float, a Rational and a Complex whose imaginary part is zero
/// (`mrb_complex_to_f`, RangeError otherwise) become a Float; anything else is its TypeError.
/// It sends nothing — a class with a `to_f` is not converted. Every Float operand of rc2's
/// numeric methods, `Math`, `sprintf`'s `%f` and `mrb_get_args`' `f` comes through here, so
/// they all say the same thing.
pub(crate) fn ensure_f64(vm: &mut Vm, v: Value) -> VmResult<f64> {
    if let Some(f) = num_f64(vm, v) { return Ok(f); }
    if super::ext_complex::is_complex(vm, v) { return super::ext_complex::to_f64(vm, v); }
    Err(not_float(vm, v))
}

/// `mrb_ensure_float_type`'s TypeError: "can't convert nil into Float" for nil, "%Y cannot be
/// converted to Float" for anything else.
#[cold]
#[inline(never)]
fn not_float(vm: &mut Vm, v: Value) -> VmError {
    if v.is_nil() { return vm.raise_type("can't convert nil into Float"); }
    let d = vm.describe_for_type_error(v);
    vm.raise_type(&format!("{d} cannot be converted to Float"))
}

/// Whether mruby 4.1.0-rc2 (64-bit word boxing, its default) keeps `f` in the value word rather
/// than in a heap object (`mrb_word_boxing_float_value`, src/etc.c). Only messages depend on it:
/// `mrb_check_type` writes an immediate with `to_s` and a heap object with its class name. The
/// constants are the reference's encoding, not choices: ±0.0 and ±Infinity have sentinels, a
/// NaN and a subnormal go to the heap, and a biased exponent in 768..=1279 is rotated into the
/// word unless the rotation lands on a sentinel.
pub(crate) fn float_is_immediate(f: f64) -> bool {
    // WORDBOX_FLOAT_EXP_MIN/MAX, WORDBOX_FLOAT_ADDEND (`(768 - (2 << 9)) << 52`),
    // WORDBOX_FLOAT_ROTATE and WORDBOX_FLOAT_SENTINEL_MAX of src/etc.c
    const EXP_MIN: u64 = 1023 - 255;
    const EXP_MAX: u64 = 1023 + 256;
    const ADDEND: u64 = ((EXP_MIN as i64 - (2 << 9)) as u64) << 52;
    const SENTINEL_MAX: u64 = 0x0e;
    let bits = f.to_bits();
    let exp = (bits >> 52) & 0x7ff;
    if exp == 0 { return bits << 1 == 0; }
    if exp == 0x7ff { return f.is_infinite(); }
    (EXP_MIN..=EXP_MAX).contains(&exp) && bits.wrapping_sub(ADDEND).rotate_left(3) > SENTINEL_MAX
}

/// `mrb_int_noconv` (src/numeric.c): how `Integer#*`, `/` and the bit operations of a small
/// receiver refuse an operand that is not a number they take, "can't convert %Y into Integer".
#[cold]
#[inline(never)]
pub(crate) fn int_noconv(vm: &mut Vm, other: Value) -> VmError {
    let d = vm.describe_for_type_error(other);
    vm.raise_type(&format!("can't convert {d} into Integer"))
}

/// `mrb_as_int`'s message, raised by the operations a wide integer takes part in.
#[cold]
#[inline(never)]
pub(crate) fn not_int(vm: &mut Vm, other: Value) -> VmError {
    let d = vm.describe_for_type_error(other);
    vm.raise_type(&format!("{d} cannot be converted to Integer"))
}

/// `mrb_as_int` (`mrb_ensure_int_type`, src/object.c of mruby 4.1.0-rc2), which is what
/// [`Vm::expect_int`] reads.
pub(crate) fn as_int(vm: &mut Vm, v: Value) -> VmResult<i64> { vm.expect_int(v, "Integer") }

#[cold]
#[inline(never)]
pub(crate) fn float_out_of_range(vm: &mut Vm, f: f64) -> VmError {
    let s = float_to_s(f);
    vm.raise(vm.core.range_error, &format!("float {s} out of range"))
}

/// `mrb_bint_as_int`'s RangeError, and `mrb_float_to_integer`'s for a Float past `mrb_int`.
#[cold]
#[inline(never)]
pub(crate) fn int_out_of_range(vm: &mut Vm) -> VmError {
    vm.raise(vm.core.range_error, "integer out of range")
}

fn zero_div(vm: &mut Vm) -> VmError {
    vm.raise(vm.core.zero_division_error, "divided by 0")
}

// ------------------------------------------------------------------ wide integers

/// The receiver of an Integer method as a [`BigInt`] (it is an Integer either way).
fn big(vm: &Vm, v: Value) -> BigInt {
    vm.as_bigint(v).unwrap_or_else(BigInt::zero)
}

/// `mrb_as_float`: an Integer of any width, or a Float.
pub(crate) fn num_f64(vm: &Vm, v: Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(i as f64),
        Value::Float(f) => Some(f),
        _ if super::ext_rational::is_rational(vm, v) => Some(super::ext_rational::to_f(vm, v)),
        Value::Obj(o) => vm.heap.bigint(o).map(|b| b.to_f64()),
        _ => None,
    }
}

// --------------------------------------------- Rational and Complex (the rest of the tower)

/// The arms `numeric.c` has for a Rational or a Complex operand: the pair is handed to the
/// gem that owns the wider type. `None` when the operand is neither.
fn zero_like(x: Value) -> Value {
    // `mrb_complex_new(a, 0)` builds the two-Float form, so a Float receiver keeps Float parts
    if matches!(x, Value::Float(_)) { Value::Float(0.0) } else { Value::Int(0) }
}

fn tower(vm: &mut Vm, x: Value, y: Value, op: char) -> VmResult<Option<Value>> {
    use super::{ext_complex as cpx, ext_rational as rat};
    if !matches!(y, Value::Obj(_)) { return Ok(None); }
    if rat::is_rational(vm, y) && !matches!(x, Value::Float(_)) {
        // a Float receiver keeps the Float arms: `1.0 + Rational(1, 2)` is 1.5
        return Ok(Some(match op {
            '+' => rat::add(vm, y, x)?,
            '*' => rat::mul(vm, y, x)?,
            '-' => { let r = rat::as_rational(vm, x)?; rat::sub(vm, r, y)? }
            _ => { let r = rat::as_rational(vm, x)?; rat::div(vm, r, y)? }
        }));
    }
    if cpx::is_complex(vm, y) {
        return Ok(Some(match op {
            '+' => cpx::arith(vm, y, x, '+')?,
            '*' => cpx::arith(vm, y, x, '*')?,
            '-' => { let c = cpx::new_complex(vm, x, zero_like(x))?; cpx::arith(vm, c, y, '-')? }
            _ => { let c = cpx::new_complex(vm, x, zero_like(x))?; cpx::div(vm, c, y)? }
        }));
    }
    Ok(None)
}

/// `mrb_bint_cmp` against a Float: the float is split at its integer part, which is exact,
/// so the fraction alone decides once the integer parts are equal.
fn bint_float_cmp(b: &BigInt, f: f64) -> Option<core::cmp::Ordering> {
    use core::cmp::Ordering::*;
    if f.is_nan() { return None; }
    if f.is_infinite() { return Some(if f < 0.0 { Greater } else { Less }); }
    let fi = libm::trunc(f);
    let c = b.cmp(&BigInt::from_f64(fi));
    if c != Equal { return Some(c); }
    Some(if f > fi { Less } else if f < fi { Greater } else { Equal })
}

/// The three operations that promote on overflow (`mrb_int_add`/`sub`/`mul`).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum IntOp { Add, Sub, Mul }

fn int_arith(vm: &mut Vm, s: Value, a: &[Value], op: IntOp) -> VmResult<Value> {
    let (x, y) = num_args(vm, s, a)?;
    if let (Value::Int(p), Value::Int(q)) = (x, y) {
        let r = match op { IntOp::Add => p.checked_add(q), IntOp::Sub => p.checked_sub(q), IntOp::Mul => p.checked_mul(q) };
        if let Some(r) = r { return Ok(Value::Int(r)); }
        return Ok(bint_arith(vm, &BigInt::from_i64(p), &BigInt::from_i64(q), op));
    }
    if let Some(v) = tower(vm, x, y, match op { IntOp::Add => '+', IntOp::Sub => '-', IntOp::Mul => '*' })? { return Ok(v); }
    if let Value::Float(q) = y {
        let p = num_f64(vm, x).unwrap_or(0.0);
        return Ok(Value::Float(match op { IntOp::Add => p + q, IntOp::Sub => p - q, IntOp::Mul => p * q }));
    }
    match vm.as_bigint(y) {
        Some(q) => { let p = big(vm, x); Ok(bint_arith(vm, &p, &q, op)) }
        None if vm.is_bigint(x) => Err(not_int(vm, y)),
        // `mrb_int_mul` refuses the rest (`mrb_int_noconv`); `mrb_int_add`/`sub` take it
        // through `mrb_as_float`
        None if op == IntOp::Mul => Err(int_noconv(vm, y)),
        None => {
            let (p, q) = (num_f64(vm, x).unwrap_or(0.0), ensure_f64(vm, y)?);
            Ok(Value::Float(if op == IntOp::Add { p + q } else { p - q }))
        }
    }
}

/// The promoted `+`/`-`/`*`, normalized back to a `Value::Int` when it fits.
pub(crate) fn bint_arith(vm: &mut Vm, p: &BigInt, q: &BigInt, op: IntOp) -> Value {
    let r = match op { IntOp::Add => p.add(q), IntOp::Sub => p.sub(q), IntOp::Mul => p.mul(q) };
    vm.bint_value(r)
}

/// `OP_ADD`/`OP_SUB`/`OP_MUL`'s overflow exit (`L_INT_OVERFLOW`): the VM's fast path keeps
/// its `checked_*` and lands here only when the result left the `i64` range.
pub fn int_overflow_op(vm: &mut Vm, p: i64, q: i64, mid: crate::symbol::Sym) -> Value {
    let s = vm.s;
    let op = if mid == s.minus { IntOp::Sub } else if mid == s.mul { IntOp::Mul } else { IntOp::Add };
    bint_arith(vm, &BigInt::from_i64(p), &BigInt::from_i64(q), op)
}

/// `Integer#/`: floor division, promoting `MRB_INT_MIN / -1`.
fn int_div(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let (x, y) = num_args(vm, s, a)?;
    if let (Value::Int(p), Value::Int(q)) = (x, y) {
        if q == 0 { return Err(zero_div(vm)); }
        if p == i64::MIN && q == -1 { return Ok(vm.bint_value(BigInt::from_i64(p).neg())); }
        return Ok(Value::Int(div_floor(p, q)));
    }
    if let Some(v) = tower(vm, x, y, '/')? { return Ok(v); }
    if let Value::Float(q) = y {
        let p = num_f64(vm, x).unwrap_or(0.0);
        return Ok(Value::Float(p / q));
    }
    // `int_div` hands a wide receiver to `mrb_bint_div` only for an Integer divisor; anything
    // else falls to the `default:` arm (`mrb_int_noconv`), whatever the receiver's width
    if !matches!(y, Value::Int(_)) && !vm.is_bigint(y) { return Err(int_noconv(vm, y)); }
    bint_div_floor(vm, x, y)
}

fn bint_div_floor(vm: &mut Vm, x: Value, y: Value) -> VmResult<Value> {
    match vm.as_bigint(y) {
        Some(q) => {
            if q.is_zero() { return Err(zero_div(vm)); }
            let p = big(vm, x);
            Ok(vm.bint_value(p.div_floor(&q)))
        }
        None if vm.is_bigint(x) => Err(not_int(vm, y)),
        // `int_div`'s `default:` arm
        None => Err(int_noconv(vm, y)),
    }
}

fn int_idiv(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let (x, y) = num_args(vm, s, a)?;
    // `int_idiv` turns the divisor of a small receiver into a machine integer (`mrb_as_int`):
    // a NaN, an infinity and a wide Integer are its RangeError (`7.div(2**70)`, the author's
    // decision of 2026-10-18, docs/design/compatibility.md). A finite Float would be truncated
    // (`7.div(1.5)` is 7 on rc2), a silently wrong answer, so it keeps the floor division below
    if let Value::Int(_) = x {
        match y {
            Value::Float(q) if !q.is_finite() => { as_int(vm, y)?; }
            Value::Obj(_) if vm.is_bigint(y) => { as_int(vm, y)?; }
            _ => {}
        }
    }
    match (x, y) {
        (Value::Int(p), Value::Int(q)) => {
            if q == 0 { return Err(zero_div(vm)); }
            if p == i64::MIN && q == -1 { return Ok(vm.bint_value(BigInt::from_i64(p).neg())); }
            Ok(Value::Int(div_floor(p, q)))
        }
        // the reference's `mrb_bint_div` multiplies by a Float instead of dividing; the
        // floor division is taken here (`docs/design/gems.md`, `tests/custom/bigint_div_float`)
        _ if float_arm(vm, y) => {
            let (p, q) = (num_f64(vm, x).unwrap_or(0.0), num_f64(vm, y).unwrap_or(0.0));
            if q == 0.0 { return Err(zero_div(vm)); }
            let d = libm::floor(p / q);
            Ok(if (-9223372036854775808.0..9223372036854775808.0).contains(&d) { Value::Int(d as i64) } else { vm.bint_value(BigInt::from_f64(d)) })
        }
        // anything else a small receiver is given goes through `mrb_as_int` (a Complex whose
        // imaginary part is zero converts, the rest is its TypeError)
        (Value::Int(p), _) => {
            let q = as_int(vm, y)?;
            if q == 0 { return Err(zero_div(vm)); }
            if p == i64::MIN && q == -1 { return Ok(vm.bint_value(BigInt::from_i64(p).neg())); }
            Ok(Value::Int(div_floor(p, q)))
        }
        _ => bint_div_floor(vm, x, y),
    }
}

/// True where `mrb_as_float` would take over: a Float, or a Rational, which converts
/// (`Integer#%`, `div` and `divmod` have no exact arm for one).
fn float_arm(vm: &Vm, y: Value) -> bool {
    matches!(y, Value::Float(_)) || super::ext_rational::is_rational(vm, y)
}

/// `int_mod` and `int_divmod` of a small receiver: what is not an Integer of either width goes
/// to `flodivmod` through `mrb_as_float` (whose TypeError is the answer for a non-number).
fn small_float_arm(vm: &Vm, x: Value, y: Value) -> bool {
    float_arm(vm, y) || (matches!(x, Value::Int(_)) && !matches!(y, Value::Int(_)) && !vm.is_bigint(y))
}

fn int_mod(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    // mruby 4.1.0-rc2 `int_mod` answers a zero receiver with itself before it looks at the
    // divisor (`if (a == 0) return x;`), unless the divisor is a big Integer. 891d1d9d took that
    // answer out (80747a5f6, master only); SabiRuby keeps rc2's.
    if s == Value::Int(0) && a.len() == 1 && !vm.is_bigint(a[0]) { return Ok(s); }
    let (x, y) = num_args(vm, s, a)?;
    match (x, y) {
        (Value::Int(p), Value::Int(q)) => {
            if q == 0 { return Err(zero_div(vm)); }
            if p == i64::MIN && q == -1 { return Ok(Value::Int(0)); }
            Ok(Value::Int(mod_floor(p, q)))
        }
        // as for `div`, the floor remainder rather than the reference's `fmod`
        _ if small_float_arm(vm, x, y) => {
            let (p, q) = (num_f64(vm, x).unwrap_or(0.0), ensure_f64(vm, y)?);
            // `flodivmod` raises for a zero divisor (rc2 and 891d1d9d alike)
            if q == 0.0 { return Err(zero_div(vm)); }
            // a zero remainder is +0.0 (`flodivmod`: `if (mod == 0) mod = 0.0`)
            let m = p % q;
            Ok(Value::Float(if m == 0.0 { 0.0 } else if (m < 0.0) != (q < 0.0) { m + q } else { m }))
        }
        _ => match vm.as_bigint(y) {
            Some(q) => {
                if q.is_zero() { return Err(zero_div(vm)); }
                let p = big(vm, x);
                Ok(vm.bint_value(p.mod_floor(&q)))
            }
            None => Err(not_int(vm, y)),
        },
    }
}

fn int_divmod(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let (x, y) = num_args(vm, s, a)?;
    match (x, y) {
        (Value::Int(p), Value::Int(q)) => {
            if q == 0 { return Err(zero_div(vm)); }
            if p == i64::MIN && q == -1 { let d = vm.bint_value(BigInt::from_i64(p).neg()); return Ok(vm.ary_new(vec![d, Value::Int(0)])); }
            Ok(vm.ary_new(vec![Value::Int(div_floor(p, q)), Value::Int(mod_floor(p, q))]))
        }
        _ if small_float_arm(vm, x, y) => {
            let (p, q) = (num_f64(vm, x).unwrap_or(0.0), ensure_f64(vm, y)?);
            let (d, m) = flodivmod(vm, p, q)?;
            let dv = if d.is_finite() { float_to_int(vm, d) } else { Value::Float(d) };
            Ok(vm.ary_new(vec![dv, Value::Float(m)]))
        }
        _ => match vm.as_bigint(y) {
            Some(q) => {
                if q.is_zero() { return Err(zero_div(vm)); }
                let p = big(vm, x);
                let (d, m) = p.divmod_floor(&q);
                let (d, m) = (vm.bint_value(d), vm.bint_value(m));
                Ok(vm.ary_new(vec![d, m]))
            }
            None => Err(not_int(vm, y)),
        },
    }
}

/// Exact Integer/Float comparison (numeric.c `mrb_int_float_cmp`): `None` for a NaN.
pub fn int_float_cmp(x: i64, y: f64) -> Option<core::cmp::Ordering> {
    use core::cmp::Ordering::*;
    if y.is_nan() { return None; }
    if y < -9223372036854775808.0 { return Some(Greater); }
    if y >= 9223372036854775808.0 { return Some(Less); }
    let yi = y as i64;
    if x > yi { return Some(Greater); }
    if x < yi { return Some(Less); }
    if y > yi as f64 { return Some(Less); }
    if y < yi as f64 { return Some(Greater); }
    Some(Equal)
}

/// `Ok(Some)` ordered, `Ok(None)` numeric but unordered (NaN), `Err` not comparable.
/// The receiver is an Integer (of either width) or a Float: only their tables call it.
fn cmp(vm: &mut Vm, s: Value, a: &[Value]) -> VmResult<Result<Option<core::cmp::Ordering>, ()>> {
    argc!(vm, a, 1);
    let (x, y) = (s, a[0]);
    if super::ext_rational::is_rational(vm, y) {
        // `cmpnum`: a Rational is compared as a Float
        let (p, q) = (num_f64(vm, x), num_f64(vm, y));
        return Ok(match (p, q) { (Some(p), Some(q)) => Ok(p.partial_cmp(&q)), _ => Err(()) });
    }
    if super::ext_complex::is_complex(vm, y) {
        // the default arm: ask the other operand and reverse the answer
        let cmp = vm.intern("<=>");
        let r = vm.funcall(y, cmp, &[x], Value::Nil)?;
        return Ok(match r { Value::Int(i) => Ok(Some(match i.cmp(&0) { core::cmp::Ordering::Less => core::cmp::Ordering::Greater, core::cmp::Ordering::Equal => core::cmp::Ordering::Equal, core::cmp::Ordering::Greater => core::cmp::Ordering::Less })), _ => Err(()) });
    }
    if vm.is_bigint(x) || vm.is_bigint(y) {
        // the reference takes the wide value first and reverses the answer for `Float <=> big`
        if let Value::Float(f) = x {
            let q = match vm.as_bigint(y) { Some(q) => q, None => return Ok(Err(())) };
            return Ok(Ok(bint_float_cmp(&q, f).map(|o| o.reverse())));
        }
        let p = match vm.as_bigint(x) { Some(p) => p, None => return Ok(Err(())) };
        return Ok(match y {
            Value::Float(f) => Ok(bint_float_cmp(&p, f)),
            _ => match vm.as_bigint(y) { Some(q) => Ok(Some(p.cmp(&q))), None => Err(()) },
        });
    }
    Ok(match (x, y) {
        (Value::Int(p), Value::Int(q)) => Ok(Some(p.cmp(&q))),
        (Value::Int(p), Value::Float(q)) => Ok(int_float_cmp(p, q)),
        (Value::Float(p), Value::Int(q)) => Ok(int_float_cmp(q, p).map(|o| o.reverse())),
        (Value::Float(p), Value::Float(q)) => Ok(p.partial_cmp(&q)),
        _ => Err(()),
    })
}
/// For `<` and friends: unordered numbers answer false, non-numbers raise.
/// `int_equal` / `flo_eq`: a Rational or a Complex on the right answers the comparison
/// itself (a Float and a Rational compare as Floats).
fn num_eq(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let y = a[0];
    if matches!(y, Value::Obj(_)) {
        let rational = super::ext_rational::is_rational(vm, y);
        if rational && matches!(s, Value::Float(_)) {
            return Ok(Value::bool(num_f64(vm, s) == num_f64(vm, y)));
        }
        if rational || super::ext_complex::is_complex(vm, y) {
            let eq = vm.s.eq;
            return vm.funcall(y, eq, &[s], Value::Nil);
        }
    }
    Ok(Value::bool(cmp(vm, s, a)? == Ok(Some(core::cmp::Ordering::Equal))))
}

fn cmp_or_fail(vm: &mut Vm, s: Value, a: &[Value]) -> VmResult<Option<core::cmp::Ordering>> {
    match cmp(vm, s, a)? {
        Ok(o) => Ok(o),
        Err(()) => { let x = vm.describe_for_error(s); let y = vm.describe_for_error(a[0]); Err(vm.raise_arg(&format!("comparison of {x} with {y} failed"))) }
    }
}
fn ord_test(o: Option<core::cmp::Ordering>, f: fn(core::cmp::Ordering) -> bool) -> Value { Value::bool(o.map(f).unwrap_or(false)) }
fn num_cmp_m(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> { Ok(match cmp(vm, s, a)? { Ok(Some(o)) => Value::Int(o as i64), _ => Value::Nil }) }
fn num_lt(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> { let o = cmp_or_fail(vm, s, a)?; Ok(ord_test(o, |o| o.is_lt())) }
fn num_le(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> { let o = cmp_or_fail(vm, s, a)?; Ok(ord_test(o, |o| o.is_le())) }
fn num_gt(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> { let o = cmp_or_fail(vm, s, a)?; Ok(ord_test(o, |o| o.is_gt())) }
fn num_ge(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> { let o = cmp_or_fail(vm, s, a)?; Ok(ord_test(o, |o| o.is_ge())) }

/// `Numeric#-@`, which mrblib numeric.rb writes as `def -@; -1 * self; end`, registered over
/// it after mrblib (`Vm::load_mrblib`) for its speed: as Ruby, `-v` is a method frame, five
/// instructions more, and bm_ao_render took +2..3% (worklog 2026-10-18 rc2-sweep-fixes Y3). An
/// Integer or a Float receiver is what `OP_MUL` answers there without a send; anything else is
/// sent `*` by -1 as the Ruby does.
pub fn post_mrblib(vm: &mut Vm) {
    let numeric = vm.core.numeric;
    vm.define_methods_args(numeric, &[("-@", Aspec::none(), num_uminus)]);
}

fn num_uminus(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    match s {
        Value::Int(i) => Ok(match i.checked_neg() { Some(r) => Value::Int(r), None => vm.bint_value(BigInt::from_i64(i).neg()) }),
        Value::Float(f) => Ok(Value::Float(-1.0 * f)),
        _ => { let mul = vm.s.mul; vm.funcall(Value::Int(-1), mul, &[s], Value::Nil) }
    }
}

/// `num_eql` (src/numeric.c): the class as well as the value. A wide Integer takes an Integer
/// of either width, a Float a Float, an Integer an Integer; any other Numeric (Rational, a
/// subclass) answers `==` when both are of the same type.
fn num_eql(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let y = a[0];
    if vm.is_bigint(s) {
        if !matches!(y, Value::Int(_)) && !vm.is_bigint(y) { return Ok(Value::False); }
        return Ok(Value::bool(cmp(vm, s, &[y])? == Ok(Some(core::cmp::Ordering::Equal))));
    }
    match (s, y) {
        (Value::Float(p), Value::Float(q)) => return Ok(Value::bool(p == q)),
        (Value::Float(_), _) => return Ok(Value::False),
        (Value::Int(p), Value::Int(q)) => return Ok(Value::bool(p == q)),
        (Value::Int(_), _) => return Ok(Value::False),
        _ => {}
    }
    // `mrb_type(x) != mrb_type(y)`
    let same = match (s, y) {
        (Value::Obj(p), Value::Obj(q)) => {
            let (r1, r2) = (super::ext_rational::is_rational(vm, s), super::ext_rational::is_rational(vm, y));
            let (c1, c2) = (super::ext_complex::is_complex(vm, s), super::ext_complex::is_complex(vm, y));
            r1 == r2 && c1 == c2 && !vm.is_bigint(y)
                && core::mem::discriminant(&vm.heap.get(p).kind) == core::mem::discriminant(&vm.heap.get(q).kind)
        }
        _ => false,
    };
    if !same { return Ok(Value::False); }
    Ok(Value::bool(vm.equal(s, y)?))
}

/// The exponent of `**`: an `i64`, or a RangeError for one that is itself wide.
fn pow_exp(vm: &mut Vm, y: Value) -> VmResult<i64> {
    match y {
        Value::Int(e) => Ok(e),
        _ if vm.is_bigint(y) => Err(vm.raise(vm.core.range_error, "integer out of range")),
        // `mrb_as_int`: a Rational truncates, anything else is a TypeError
        Value::Obj(_) => vm.expect_int(y, "exponent"),
        _ => { let d = vm.describe_for_type_error(y); Err(vm.raise_type(&format!("{d} cannot be converted to Integer"))) }
    }
}

/// `mrb_bint_pow`'s TypeError for an exponent that is no Integer: "%Y cannot be convert to
/// integer", spelled as the reference spells it (wording follows the reference).
#[cold]
#[inline(never)]
fn bint_pow_type(vm: &mut Vm, y: Value) -> VmError {
    let d = vm.describe_for_type_error(y);
    vm.raise_type(&format!("{d} cannot be convert to integer"))
}

/// The reference's cap on `mrb_bint_pow` (`MRB_BIGINT_POW_MAX_BITS`).
const POW_MAX_BITS: u64 = 1000000;

/// `mrb_bint_pow`: a wide base, with the reference's guard against a huge result.
fn bint_pow(vm: &mut Vm, base: BigInt, e: i64) -> VmResult<Value> {
    if e < 0 { return Err(vm.raise_arg("negative exponent")); }
    let bits = base.bit_length().max(1);
    if e > 0 && e as u64 > POW_MAX_BITS / bits { return Err(vm.raise(vm.core.range_error, "exponent too large")); }
    Ok(vm.bint_value(base.pow(e as u64)))
}

pub(crate) fn int_pow(vm: &mut Vm, s: Value, a: &[Value]) -> VmResult<Value> {
    let (x, y) = num_args(vm, s, a)?;
    if vm.is_bigint(x) {
        if let Value::Float(q) = y { return Ok(Value::Float(libm::pow(num_f64(vm, x).unwrap_or(0.0), q))); }
        // `mrb_bint_pow`: a wide exponent of a wide base is refused before anything else
        if vm.is_bigint(y) { return Err(vm.raise_type("too big power")); }
        // and anything that is not an Integer after it, with the reference's wording (sic)
        let e = match y { Value::Int(e) => e, _ => return Err(bint_pow_type(vm, y)) };
        let b = big(vm, x);
        return bint_pow(vm, b, e);
    }
    let p = match x { Value::Int(p) => p, _ => 0 };
    if let Value::Float(q) = y { return Ok(Value::Float(libm::pow(p as f64, q))); }
    let e = pow_exp(vm, y)?;
    if e < 0 { return Ok(Value::Float(libm::pow(p as f64, e as f64))); }
    match (|| { let mut r: i64 = 1; let mut b = p; let mut e = e; loop { if e & 1 == 1 { r = r.checked_mul(b)?; } e >>= 1; if e == 0 { break; } b = b.checked_mul(b)?; } Some(r) })() {
        Some(r) => Ok(Value::Int(r)),
        None => bint_pow(vm, BigInt::from_i64(p), e),
    }
}

/// `Integer#to_s(base)`, for both widths.
fn int_to_s(vm: &mut Vm, s: Value, a: &[Value], _b: Value) -> VmResult<Value> {
    let base = if a.len() == 1 { vm.expect_int(a[0], "base")? } else { 10 };
    if !(2..=36).contains(&base) { return Err(vm.raise_arg(&format!("invalid radix {base}"))); }
    if let Value::Int(i) = s {
        let out = if base == 10 { i.to_string() } else { to_radix(i, base as u32) };
        return Ok(vm.str_from(out));
    }
    let out = big(vm, s).to_string_radix(base as u32);
    Ok(vm.str_from(out))
}
fn to_radix(i: i64, base: u32) -> String {
    if i == 0 { return "0".into(); }
    let neg = i < 0;
    let mut n = (i as i128).unsigned_abs();
    let mut digits = vec![];
    while n > 0 { digits.push(core::char::from_digit((n % base as u128) as u32, base).unwrap()); n /= base as u128; }
    if neg { digits.push('-'); }
    digits.iter().rev().collect()
}

/// `mrb_float_to_integer` (src/numeric.c of mruby 4.1.0-rc2), what `Integer()` does with a
/// Float: RangeError `float NaN out of range` for a NaN or an infinity (where `Float#to_i` says
/// FloatDomainError), a wide Integer past `i64`.
pub(crate) fn float_to_integer(vm: &mut Vm, f: f64) -> VmResult<Value> {
    if f.is_nan() || f.is_infinite() { return Err(float_out_of_range(vm, f)); }
    Ok(float_to_int(vm, libm::trunc(f)))
}

/// `Float#to_i` and friends: a value beyond `i64` becomes a wide integer.
pub(crate) fn float_to_int(vm: &mut Vm, f: f64) -> Value {
    if (-9223372036854775808.0..9223372036854775808.0).contains(&f) { Value::Int(f as i64) } else { vm.bint_value(BigInt::from_f64(f)) }
}

fn bit(vm: &mut Vm, s: Value, a: &[Value], f: fn(i64, i64) -> i64, fb: fn(&BigInt, &BigInt) -> BigInt) -> VmResult<Value> {
    let (x, y) = num_args(vm, s, a)?;
    if let (Value::Int(p), Value::Int(q)) = (x, y) { return Ok(Value::Int(f(p, q))); }
    // `mrb_bint_and` and the others take an operand that is not an Integer with `mrb_as_bint`
    // (`mrb_as_int`): a NaN or an infinity is its RangeError. A finite Float would be truncated
    // (`(2**70) & 1.5` is 0 on rc2), a silently wrong answer, so it stays the TypeError below
    if let Value::Float(q) = y {
        if vm.is_bigint(x) {
            if !q.is_finite() { super::numeric::as_int(vm, y)?; }
            // the refusal `bit_op` gives a Float with a small receiver (`mrb_int_noconv`)
            return Err(vm.raise_type("can't convert Float into Integer"));
        }
    }
    match vm.as_bigint(y) {
        Some(q) => { let p = big(vm, x); Ok(vm.bint_value(fb(&p, &q))) }
        // `mrb_as_bint` of a wide receiver: a value that is no number is `mrb_as_int`'s TypeError;
        // a Rational or a Complex would be truncated, so it is refused as the Float above is
        None if vm.is_bigint(x) && !super::ext_rational::is_rational(vm, y) && !super::ext_complex::is_complex(vm, y) => Err(not_int(vm, y)),
        // `bit_op` of a small receiver (`mrb_int_noconv`)
        None => Err(int_noconv(vm, y)),
    }
}

fn shift(vm: &mut Vm, s: Value, a: &[Value], left: bool) -> VmResult<Value> {
    let (x, y) = num_args(vm, s, a)?;
    // `int_lshift`/`int_rshift` read the width with `mrb_as_int` (a Float truncates)
    let w = match y { Value::Int(q) => q, _ => vm.expect_int(y, "width")? };
    let (left, n) = if w < 0 { (!left, w.unsigned_abs()) } else { (left, w as u64) };
    if let Value::Int(p) = x {
        if !left { return Ok(Value::Int(if n >= 64 { if p < 0 { -1 } else { 0 } } else { p >> n })); }
        if p == 0 { return Ok(Value::Int(0)); }
        if n < 64 {
            if let Some(r) = p.checked_shl(n as u32) { if (r >> n) == p { return Ok(Value::Int(r)); } }
        }
    }
    let p = big(vm, x);
    // the reference lets the allocation fail; a width that cannot be held is refused here
    if left && n > MAX_SHIFT { return Err(vm.raise(vm.core.range_error, "shift width too big")); }
    Ok(vm.bint_value(if left { p.shl(n) } else { p.shr(n) }))
}

/// Widest left shift accepted, in bits (`docs/design/gems.md`: the reference has no limit and
/// fails in `mrb_realloc` instead).
const MAX_SHIFT: u64 = 1 << 26;

pub fn init(vm: &mut Vm) {
    let c = vm.core;
    // EPSILON, MAX, MIN, DIG, ... are mruby-numeric-ext (`ext_numeric.rs`)
    for (name, v) in [("INFINITY", f64::INFINITY), ("NAN", f64::NAN)] {
        let n = vm.intern(name);
        vm.heap.class_mut(c.float).consts.insert(n, Slot::from(Value::Float(v)));
    }
    let isc = vm.singleton_class(Value::Obj(c.integer)).unwrap();
    // `int_s_ensure`: `mrb_ensure_int_type`, so a wide Integer is narrowed (or refused) as well
    vm.define_method_args(isc, "__ensure", Aspec::req(1), |vm, _s, a, _b| { match a[0] { Value::Int(_) => Ok(a[0]), v => Ok(Value::Int(vm.expect_int(v, "Integer")?)) } });
    // `numeric_rom_entries` (src/numeric.c): Numeric has `finite?`, `infinite?`, `eql?` and
    // `fdiv` of its own. Its `<`, `<=`, `>`, `>=`, `==` and `between?` are Comparable's (mrblib
    // compar.rb, which `include Comparable` in mrblib numeric.rb brings in), and Integer and
    // Float define `<=>` and the four comparisons themselves (below).
    vm.define_methods_args(c.numeric, &[
        ("eql?", Aspec::req(1), num_eql),
        // a Complex with an infinite part is 1, as CRuby's `Complex#infinite?` (rc2 answers false
        // for every Complex through `num_infinite_p`: README, "Known deviations")
        ("infinite?", Aspec::none(), |vm, s, _a, _b| {
            if let Some((re, im)) = super::ext_complex::parts(vm, s) {
                return Ok(if [re, im].iter().any(|p| matches!(p, Value::Float(f) if f.is_infinite())) { Value::Int(1) } else { Value::Nil });
            }
            Ok(match s { Value::Float(f) if f.is_infinite() => Value::Int(if f > 0.0 { 1 } else { -1 }), _ => Value::Nil })
        }),
        ("finite?", Aspec::none(), |_vm, s, _a, _b| { Ok(Value::bool(!matches!(s, Value::Float(f) if !f.is_finite()))) }),
        // `num_fdiv` (src/numeric.c): the receiver goes through `mrb_ensure_float_type`, which
        // converts the numeric types and raises for anything else — it does not send `to_f` —
        // and then it is ordinary Float division. Integer and Float have their own `fdiv`; this
        // is the one the rest of the tower (Rational, and a wide Integer) answers with.
        ("fdiv", Aspec::req(1), |vm, s, a, _b| {
            let p = ensure_f64(vm, s)?;
            let q = ensure_f64(vm, a[0])?;
            Ok(Value::Float(p / q))
        }),
    ]);
    vm.define_methods_args(c.integer, &[
        // `integer_rom_entries`: `num_cmp`, `num_lt`, ... (shared with Float's table)
        ("<=>", Aspec::req(1), num_cmp_m),
        ("<", Aspec::req(1), num_lt),
        ("<=", Aspec::req(1), num_le),
        (">", Aspec::req(1), num_gt),
        (">=", Aspec::req(1), num_ge),
        ("+", Aspec::req(1), |vm, s, a, _b| int_arith(vm, s, a, IntOp::Add)),
        ("-", Aspec::req(1), |vm, s, a, _b| int_arith(vm, s, a, IntOp::Sub)),
        ("*", Aspec::req(1), |vm, s, a, _b| int_arith(vm, s, a, IntOp::Mul)),
        ("/", Aspec::req(1), int_div),
        ("div", Aspec::req(1), int_idiv),
        ("%", Aspec::req(1), int_mod),
        ("**", Aspec::req(1), |vm, s, a, _b| int_pow(vm, s, a)),
        ("divmod", Aspec::req(1), int_divmod),
        ("~", Aspec::none(), |vm, s, _a, _b| { Ok(match s {
            Value::Int(i) => Value::Int(!i),
            v if vm.is_bigint(v) => { let b = big(vm, v).not(); vm.bint_value(b) }
            v => v,
        }) }),
        ("&", Aspec::req(1), |vm, s, a, _b| bit(vm, s, a, |p, q| p & q, BigInt::and)),
        ("|", Aspec::req(1), |vm, s, a, _b| bit(vm, s, a, |p, q| p | q, BigInt::or)),
        ("^", Aspec::req(1), |vm, s, a, _b| bit(vm, s, a, |p, q| p ^ q, BigInt::xor)),
        ("<<", Aspec::req(1), |vm, s, a, _b| shift(vm, s, a, true)),
        (">>", Aspec::req(1), |vm, s, a, _b| shift(vm, s, a, false)),
        ("==", Aspec::req(1), num_eq),
        ("hash", Aspec::none(), |vm, s, _a, _b| Ok(Value::Int(vm.value_hash(s)))),
        ("__coerce_step_counter", Aspec::req(1), |vm, s, a, _b| { Ok(match a[0] { Value::Float(_) => Value::Float(num_f64(vm, s).unwrap_or(0.0)), _ => s }) }),
        ("to_s", Aspec::opt(1), int_to_s),
        ("inspect", Aspec::opt(1), int_to_s),
        ("to_i", Aspec::none(), |_vm, s, _a, _b| { Ok(s) }),
        ("to_int", Aspec::none(), |_vm, s, _a, _b| { Ok(s) }),
        ("to_f", Aspec::none(), |vm, s, _a, _b| { Ok(Value::Float(num_f64(vm, s).unwrap_or(0.0))) }),
        ("chr", Aspec::opt(1), |vm, s, _a, _b| {
            let i = match s { Value::Int(i) => i, _ => return Err(vm.raise(vm.core.range_error, "integer out of range")) };
            if !(0..=255).contains(&i) { return Err(vm.raise(vm.core.range_error, &format!("{i} out of char range"))); }
            Ok(vm.str_new(&[i as u8]))
        }),
        ("floor", Aspec::opt(1), |vm, s, a, _b| int_rounding(vm, s, a, Rounding::Floor)),
        ("ceil", Aspec::opt(1), |vm, s, a, _b| int_rounding(vm, s, a, Rounding::Ceil)),
        ("round", Aspec::opt(1), |vm, s, a, _b| int_rounding(vm, s, a, Rounding::Round)),
        ("truncate", Aspec::opt(1), |vm, s, a, _b| int_rounding(vm, s, a, Rounding::Truncate)),
        // `int_quo`: with mruby-rational loaded the quotient of two Integers is exact
        ("quo", Aspec::req(1), |vm, s, a, _b| {
            let y = a[0];
            if let (Some(n), Some(d)) = (vm.as_bigint(s), vm.as_bigint(y)) { return super::ext_rational::new_rat(vm, &n, &d); }
            if super::ext_rational::is_rational(vm, y) {
                let r = super::ext_rational::as_rational(vm, s)?;
                return super::ext_rational::div(vm, r, y);
            }
            let p = num_f64(vm, s).unwrap_or(0.0);
            let q = ensure_f64(vm, y)?;
            Ok(Value::Float(p / q))
        }),
        // `int_fdiv`: the divisor through `mrb_as_float`, and a zero one (0, 0.0, -0.0) is
        // ZeroDivisionError for a receiver of either width (Float#fdiv divides on)
        ("fdiv", Aspec::req(1), |vm, s, a, _b| { let p = num_f64(vm, s).unwrap_or(0.0); let q = ensure_f64(vm, a[0])?; if q == 0.0 { return Err(zero_div(vm)); } Ok(Value::Float(p / q)) }),
    ]);
    vm.define_methods_args(c.float, &[
        // `float_rom_entries`: the comparisons as Integer's, and `flo_finite_p`/`flo_infinite_p`
        ("<=>", Aspec::req(1), num_cmp_m),
        ("<", Aspec::req(1), num_lt),
        ("<=", Aspec::req(1), num_le),
        (">", Aspec::req(1), num_gt),
        (">=", Aspec::req(1), num_ge),
        ("infinite?", Aspec::none(), |_vm, s, _a, _b| { Ok(match s { Value::Float(f) if f.is_infinite() => Value::Int(if f > 0.0 { 1 } else { -1 }), _ => Value::Nil }) }),
        ("finite?", Aspec::none(), |_vm, s, _a, _b| { Ok(Value::bool(!matches!(s, Value::Float(f) if !f.is_finite()))) }),
        // `flo_idiv`: floor division to an Integer
        ("div", Aspec::req(1), |vm, s, a, _b| { let x = match s { Value::Float(x) => x, _ => 0.0 }; if !x.is_finite() { return Err(vm.raise(vm.core.float_domain_error, &float_to_s(x))); } let y = vm.expect_int(a[0], "divisor")?; if !(-9223372036854775808.0..9223372036854775808.0).contains(&x) { return Err(vm.raise(vm.core.range_error, "integer overflow in div")); } if y == 0 { return Err(vm.raise(vm.core.zero_division_error, "divided by 0")); } Ok(Value::Int(div_floor(x as i64, y))) }),
        ("+", Aspec::req(1), |vm, s, a, _b| { if let Some(v) = tower(vm, s, a[0], '+')? { return Ok(v); } float_binop(vm, s, a, |p, q| p + q) }),
        ("-", Aspec::req(1), |vm, s, a, _b| { if let Some(v) = tower(vm, s, a[0], '-')? { return Ok(v); } float_binop(vm, s, a, |p, q| p - q) }),
        ("*", Aspec::req(1), |vm, s, a, _b| { if let Some(v) = tower(vm, s, a[0], '*')? { return Ok(v); } float_binop(vm, s, a, |p, q| p * q) }),
        ("/", Aspec::req(1), |vm, s, a, _b| { if let Some(v) = tower(vm, s, a[0], '/')? { return Ok(v); } float_binop(vm, s, a, |p, q| p / q) }),
        ("quo", Aspec::req(1), |vm, s, a, _b| float_binop(vm, s, a, |p, q| p / q)),
        ("fdiv", Aspec::req(1), |vm, s, a, _b| float_binop(vm, s, a, |p, q| p / q)),
        // `flo_mod` is `flodivmod`'s remainder, which raises for a zero divisor
        ("%", Aspec::req(1), |vm, s, a, _b| { let (x, y) = num_args(vm, s, a)?; if num_f64(vm, y) == Some(0.0) && matches!(x, Value::Float(_)) { return Err(zero_div(vm)); } float_binop(vm, s, a, |p, q| { let m = p % q; if m == 0.0 { 0.0 } else if (m < 0.0) != (q < 0.0) { m + q } else { m } }) }),
        ("**", Aspec::req(1), |vm, s, a, _b| float_binop(vm, s, a, libm::pow)),
        ("==", Aspec::req(1), num_eq),
        ("hash", Aspec::none(), |_vm, s, _a, _b| Ok(Value::Int(match s { Value::Float(f) => f.to_bits() as i64, _ => 0 }))),
        ("to_s", Aspec::none(), |vm, s, _a, _b| { let f = match s { Value::Float(f) => f, _ => 0.0 }; Ok(vm.str_from(float_to_s(f))) }),
        ("inspect", Aspec::none(), |vm, s, _a, _b| { let f = match s { Value::Float(f) => f, _ => 0.0 }; Ok(vm.str_from(float_to_s(f))) }),
        ("to_f", Aspec::none(), |_vm, s, _a, _b| { Ok(s) }),
        ("to_i", Aspec::none(), float_to_i),
        // `flo_nan_p`: Float's own; rc2 has no `nan?` on Numeric or Integer
        ("nan?", Aspec::none(), |_vm, s, _a, _b| { Ok(Value::bool(matches!(s, Value::Float(f) if f.is_nan()))) }),
        // `flo_truncate`: `flo_ceil` for a value whose sign bit is set, `flo_floor` otherwise
        ("truncate", Aspec::opt(1), |vm, s, a, _b| { let neg = matches!(s, Value::Float(f) if f.is_sign_negative()); float_round(vm, s, a, if neg { libm::ceil } else { libm::floor }) }),
        ("floor", Aspec::opt(1), |vm, s, a, _b| float_round(vm, s, a, libm::floor)),
        ("ceil", Aspec::opt(1), |vm, s, a, _b| float_round(vm, s, a, libm::ceil)),
        ("round", Aspec::opt(1), |vm, s, a, _b| { let nd = if a.is_empty() { 0 } else { vm.expect_int(a[0], "digits")? }; flo_round(vm, s, nd) }),
        ("abs", Aspec::none(), |_vm, s, _a, _b| { Ok(match s { Value::Float(f) => Value::Float(f.abs()), v => v }) }),
        // `flo_divmod`: the quotient comes back as an Integer when it fits
        ("divmod", Aspec::req(1), |vm, s, a, _b| { let p = match s { Value::Float(f) => f, _ => 0.0 }; let q = ensure_f64(vm, a[0])?; let (d, m) = flodivmod(vm, p, q)?; let dv = if d.is_finite() { float_to_int(vm, d) } else { Value::Float(d) }; Ok(vm.ary_new(vec![dv, Value::Float(m)])) }),
    ]);
}

/// numeric.c `flodivmod`: the remainder comes from `fmod`, which is exact, and the quotient
/// from `(x - mod) / y` rounded, so a large quotient keeps its last digits.
fn flodivmod(vm: &mut Vm, x: f64, y: f64) -> VmResult<(f64, f64)> {
    if y.is_nan() { return Ok((y, y)); }
    if y == 0.0 { return Err(zero_div(vm)); }
    let mut m = if y.is_infinite() && !x.is_infinite() { x } else { libm::fmod(x, y) };
    let mut d = if x.is_infinite() && !y.is_infinite() { x } else { libm::round((x - m) / y) };
    if d == 0.0 { d = 0.0; }
    if m == 0.0 { m = 0.0; }
    if y * m < 0.0 { m += y; d -= 1.0; }
    Ok((d, m))
}

fn float_binop(vm: &mut Vm, s: Value, a: &[Value], ff: fn(f64, f64) -> f64) -> VmResult<Value> {
    let (x, y) = num_args(vm, s, a)?;
    let p = match x { Value::Float(p) => p, _ => 0.0 };
    // `flo_add` and the others: `mrb_as_float`
    let q = ensure_f64(vm, y)?;
    Ok(Value::Float(ff(p, q)))
}

/// numeric.c `flo_round`.
fn flo_round(vm: &mut Vm, s: Value, nd: i64) -> VmResult<Value> {
    let number = match s { Value::Float(f) => f, _ => return Ok(s) };
    if nd > 0 && !number.is_finite() { return Ok(s); }
    if !number.is_finite() { return Err(vm.raise(vm.core.float_domain_error, &float_to_s(number))); }
    const DBL_DIG: i64 = 15;
    if nd < -DBL_DIG - 2 { return Ok(Value::Int(0)); }
    if nd > DBL_DIG + 2 { return Ok(s); }
    let mut f = 1.0f64;
    for _ in 0..nd.abs() { f *= 10.0; }
    let mut x = number;
    if f.is_infinite() { if nd < 0 { x = 0.0; } }
    else {
        if nd < 0 { x /= f; } else { x *= f; }
        if x > 0.0 { let d = libm::floor(x); x = d + if x - d >= 0.5 { 1.0 } else { 0.0 }; }
        else if x < 0.0 { let d = libm::ceil(x); x = d - if d - x >= 0.5 { 1.0 } else { 0.0 }; }
        if nd < 0 { x *= f; } else { x /= f; }
    }
    if nd > 0 { return Ok(if x.is_finite() { Value::Float(x) } else { s }); }
    if x.abs() >= 4611686018427387904.0 { return Ok(Value::Float(x)); }
    Ok(Value::Int(x as i64))
}

#[derive(Clone, Copy, PartialEq)]
enum Rounding { Floor, Ceil, Round, Truncate }

/// `Integer#floor/ceil/round/truncate(ndigits)` for negative `ndigits`
/// (numeric.c `prepare_int_rounding` and the wide arms of `int_floor` and friends; the
/// wide arms answer the immediate case too, which they agree with and which can overflow).
fn int_rounding(vm: &mut Vm, s: Value, a: &[Value], mode: Rounding) -> VmResult<Value> {
    argc!(vm, a, 0, 1);
    let nd = if a.is_empty() { 0 } else { vm.expect_int(a[0], "ndigits")? };
    if nd >= 0 { return Ok(s); }
    let x = match vm.as_bigint(s) { Some(x) => x, None => return Ok(s) };
    // more trailing zeros than the value has digits: the answer is 0
    let bytes = if x.mag.len() > 2 { x.byte_size() as f64 + 0.125 } else { 8.0 - 0.125 };
    if -0.415241 * nd as f64 > bytes { return Ok(Value::Int(0)); }
    let f = BigInt::from_i64(10).pow((-nd) as u64);
    let m = x.mod_floor(&f);
    let n = x.sub(&m);
    let r = match mode {
        Rounding::Floor => n,
        Rounding::Ceil => if m.is_zero() { x } else { n.add(&f) },
        Rounding::Truncate => if n.sign() < 0 { n.add(&f) } else { n },
        Rounding::Round => {
            let h = f.shr(1);
            let c = m.cmp(&h);
            if c.is_gt() || (c.is_eq() && x.sign() > 0) { n.add(&f) } else { n }
        }
    };
    Ok(vm.bint_value(r))
}

fn float_to_i(vm: &mut Vm, s: Value, _a: &[Value], _b: Value) -> VmResult<Value> {
    // `flo_to_i` is `MRB_ARGS_NONE` in the reference's method table
    match s {
        Value::Float(f) if f.is_finite() => Ok(float_to_int(vm, libm::trunc(f))),
        Value::Float(f) => Err(vm.raise(vm.core.float_domain_error, &float_to_s(f))),
        v => Ok(v),
    }
}
/// `flo_rounding` (src/numeric.c of mruby 4.1.0-rc2): `floor`/`ceil` to `ndigits` decimal
/// digits. A positive count answers a Float (the receiver itself past the digits a double
/// holds), zero or a negative one an Integer; a result that is not finite is
/// `mrb_check_num_exact`'s FloatDomainError.
fn float_round(vm: &mut Vm, s: Value, a: &[Value], func: fn(f64) -> f64) -> VmResult<Value> {
    argc!(vm, a, 0, 1);
    let f = match s { Value::Float(x) => x, v => return Ok(v) };
    let nd = if a.is_empty() { 0 } else { vm.expect_int(a[0], "ndigits")? };
    // `fprec` of the reference: DBL_DIG, the decimal digits a double holds
    let fprec = f64::DIGITS as i64;
    if f == 0.0 { return Ok(if nd > 0 { s } else { Value::Int(0) }); }
    if nd > 0 {
        if nd > fprec { return Ok(s); }
        let d = libm::pow(10.0, nd as f64);
        let r = func(f * d) / d;
        if !r.is_finite() { return Err(vm.raise(vm.core.float_domain_error, &float_to_s(r))); }
        return Ok(Value::Float(r));
    }
    let r = if nd < 0 { let d = libm::pow(10.0, -(nd as f64)); func(f / d) * d } else { func(f) };
    if !r.is_finite() { return Err(vm.raise(vm.core.float_domain_error, &float_to_s(r))); }
    Ok(float_to_int(vm, r))
}
