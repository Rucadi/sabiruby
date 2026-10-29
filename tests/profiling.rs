#![cfg(feature = "profiling")]

use core::sync::atomic::{AtomicU64, Ordering};
use sabiruby::{Value, Vm, VmError};

fn clock() -> u64 {
    static CLOCK: AtomicU64 = AtomicU64::new(0);
    CLOCK.fetch_add(200_000, Ordering::Relaxed)
}

fn inner(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Int(42))
}

fn outer(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let name = vm.intern("profile_inner");
    vm.funcall(receiver, name, &[], Value::Nil)
}

fn fail(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Err(vm.raise_arg("profiled error"))
}

#[test]
fn profiling_bounds_events_and_preserves_nested_native_and_exception_behavior() {
    let mut vm = Vm::with_mrblib().unwrap();
    vm.define_method(vm.core.object, "profile_inner", inner);
    vm.define_method(vm.core.object, "profile_outer", outer);
    vm.define_method(vm.core.object, "profile_fail", fail);
    let code = sabiruby_compiler::compile(
        br#"
      10.times { raise 'bad result' unless profile_outer == 42 }
      begin
        profile_fail
      rescue ArgumentError
        puts 'recovered'
      end
    "#,
        &sabiruby_compiler::Options {
            debug_info: true,
            filename: "profiling.rb".into(),
            ..Default::default()
        },
    )
    .unwrap();
    vm.profile = Some(sabiruby::profile::Profile::new(clock, 1, 4));
    vm.load_and_run(&code).unwrap();
    assert_eq!(vm.take_output(), b"recovered\n");
    let profile = vm.profile.as_ref().unwrap();
    assert!(profile.samples.len() + profile.spans.len() <= 4);
    assert!(profile.dropped_events > 0);
    assert!(
        profile
            .natives
            .iter()
            .all(|stats| stats.inclusive_ns >= stats.exclusive_native_ns)
    );
    assert!(profile.natives.iter().any(|stats| stats.calls >= 10));
    assert!(
        profile
            .frames
            .iter()
            .any(|frame| frame.contains("profiling.rb"))
    );
    vm.profile = None;
    vm.load_and_run(&code).unwrap();
    assert_eq!(vm.take_output(), b"recovered\n");
}
