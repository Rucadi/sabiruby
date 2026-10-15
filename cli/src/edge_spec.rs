//! `sabiruby edge-spec helpers.mrb test.mrb [--events FILE]`: one file of mruby/edge's
//! compatibility driver (`mrubyedge/tests/mruby_spec/`, BSD-3-Clause, Uchio Kondo) on a fresh
//! SabiRuby VM, so that the two VMs are counted by the same Ruby (`helpers.rb`, unmodified).
//!
//! The steps and their phase events are those of `execute` in mruby/edge's `mod.rs`: helpers,
//! then the test file (compiled with an end-of-file marker `$spec_completed = true` appended,
//! `tools/edge_compare.sh`), then `report`. The natives are the ones its `install` defines and
//! no others: `t_print`, `_str_match?` and `_spec_event` (`events.rs`). The C fixtures of the
//! reference's own driver (`sabiruby::mrbtest::install`) are deliberately left out, as they are
//! there. With `--with-fixtures` they are defined first (and the three natives above over
//! them), which is a run under conditions mruby/edge does not have. The exit status is 0 only
//! when `report` answers true; a time limit is the caller's.

use std::fs::OpenOptions;
use std::io::Write;
use std::process::ExitCode;
use std::sync::OnceLock;

use sabiruby::{Value, Vm};

/// The append-only events file (`--events`); unset, no events are written (as mruby/edge
/// without `MRUBYEDGE_SPEC_EVENTS`).
static EVENTS: OnceLock<Option<String>> = OnceLock::new();

/// One JSON line, appended and closed at once so that a killed process loses nothing written.
fn emit(line: &str) {
    if let Some(Some(path)) = EVENTS.get() {
        let mut f = OpenOptions::new().create(true).append(true).open(path).expect("cannot open spec events");
        writeln!(f, "{line}").expect("cannot write spec event");
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn phase(name: &str, label: &str) {
    emit(&format!("[\"phase\",{},{}]", json_str(name), json_str(label)));
}

/// What the VM printed so far goes out before anything a native prints, so the log keeps the order.
fn flush(vm: &mut Vm) {
    let out = std::io::stdout();
    let mut lock = out.lock();
    lock.write_all(&vm.take_output()).ok();
    lock.flush().ok();
}

fn install(vm: &mut Vm) {
    let o = vm.core.object;
    // events.rs: Integer as a number, String as a string, nil as null, anything else as text
    // (mruby/edge writes its Rust Debug form there; this writes `inspect`)
    vm.define_method(o, "_spec_event", |vm, _s, a, _b| {
        let mut items = Vec::with_capacity(a.len());
        for v in a {
            items.push(match *v {
                Value::Int(n) => n.to_string(),
                Value::Nil => "null".to_string(),
                _ => match vm.str_bytes(*v) {
                    Some(b) => json_str(&String::from_utf8_lossy(b)),
                    None => json_str(&vm.inspect_str(*v)?),
                },
            });
        }
        emit(&format!("[{}]", items.join(",")));
        Ok(Value::Nil)
    });
    // mod.rs: nil prints nothing, a String its bytes, an Integer its digits; anything else is
    // the TypeMismatch mruby/edge raises there (here the TypeError of `expect_str`)
    vm.define_method(o, "t_print", |vm, _s, a, _b| {
        flush(vm);
        let mut bytes = Vec::new();
        for v in a {
            match *v {
                Value::Nil => {}
                Value::Int(n) => bytes.extend_from_slice(n.to_string().as_bytes()),
                _ => bytes.extend_from_slice(&vm.expect_str(*v, "t_print argument")?),
            }
        }
        let out = std::io::stdout();
        let mut lock = out.lock();
        lock.write_all(&bytes).ok();
        lock.flush().ok();
        Ok(Value::Nil)
    });
    // the glob of `assert_match`: the reference driver's `str_match_p`, which SabiRuby's own
    // mrbtest already ports (mruby/edge's `pattern.rs` is a reimplementation of the same rules)
    vm.define_method(o, "_str_match?", |vm, _s, a, _b| {
        vm.check_argc(a, 2, 2)?;
        let pat = vm.expect_str(a[0], "pattern")?;
        let s = vm.expect_str(a[1], "string")?;
        Ok(Value::bool(sabiruby::mrbtest::glob_match(&pat, &s, 0)))
    });
}

/// Loads and runs one compiled file; on an error prints it (with the backtrace the error
/// carries) and answers false, after the phase `runtime_error`, as mruby/edge's `eval` does.
fn eval(vm: &mut Vm, path: &str, label: &str) -> bool {
    phase("compile", label);
    // the compilation was `mrbc`'s; a file it could not compile is missing or not RITE here
    let bin = match std::fs::read(path) {
        Ok(b) if b.starts_with(b"RITE") => b,
        _ => { eprintln!("{label}: compile failed (no bytecode at {path})"); return false; }
    };
    phase("execute", label);
    let r = vm.load_and_run(&bin);
    flush(vm);
    match r {
        Ok(_) => true,
        Err(e) => {
            phase("runtime_error", label);
            eprintln!("{label}: {}", vm.describe_error(&e));
            false
        }
    }
}

pub fn run(helpers: &str, test: &str, events: Option<String>, label: Option<String>,
           with_fixtures: bool, wall_clock: fn() -> (i64, i64)) -> ExitCode {
    EVENTS.set(events).ok();
    let label = label.unwrap_or_else(|| std::path::Path::new(test).with_extension("rb")
        .file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    // the VM as the `sabiruby` command makes it (src/main.rs `run`): mrblib with the gems,
    // the reference compiler for `eval`, a clock and `sleep`
    let mut vm = Vm::new();
    vm.set_host(Box::new(sabiruby_compiler::Compiler::new()));
    vm.wall_clock = Some(wall_clock);
    vm.sleep_hook = Some(|micros| std::thread::sleep(std::time::Duration::from_micros(micros)));
    if let Err(e) = vm.load_mrblib() { eprintln!("failed to initialize VM (mrblib): {}", vm.describe_error(&e)); return ExitCode::from(1); }
    // the C fixtures first: their `t_print` and `_str_match?` are on Kernel, the driver's on
    // Object, so the driver's are the ones called
    if with_fixtures { sabiruby::mrbtest::install(&mut vm); }
    install(&mut vm);
    if !eval(&mut vm, helpers, "helpers.rb") { return ExitCode::from(1); }
    if !eval(&mut vm, test, &label) { return ExitCode::from(1); }
    phase("returned", &label);
    if !vm.global_get("$spec_completed").truthy() {
        eprintln!("{label}: VM returned before the end of the test file");
        return ExitCode::from(1);
    }
    phase("completed", &label);
    let report = vm.intern("report");
    let top = Value::Obj(vm.top_self);
    let r = vm.funcall(top, report, &[], Value::Nil);
    flush(&mut vm);
    match r {
        Ok(v) if v.truthy() => ExitCode::SUCCESS,
        Ok(_) => { eprintln!("{label}: upstream assertions failed"); ExitCode::from(1) }
        Err(e) => { eprintln!("{label}: report failed: {}", vm.describe_error(&e)); ExitCode::from(1) }
    }
}
