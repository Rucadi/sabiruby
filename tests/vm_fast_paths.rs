//! Behaviour the VM's shortcuts must not change: long jumps for methods over 32 KiB of
//! bytecode, the `Array#length`/`size`/`[]` answers made in the send itself, and the
//! constant cache.

use sabiruby::Vm;

fn compile(src: &str) -> Vec<u8> {
    sabiruby_compiler::compile(src.as_bytes(), &sabiruby_compiler::Options {
        filename: "(test)".into(), debug_info: true, ..Default::default()
    }).expect("compile")
}

fn run(src: &str) -> String {
    let mut vm = Vm::with_mrblib().expect("vm");
    vm.set_host(Box::new(sabiruby_compiler::Compiler::new()));
    if let Err(error) = vm.load_and_run(&compile(src)) {
        panic!("{}\n{}", vm.describe_error(&error), String::from_utf8_lossy(&vm.take_output()));
    }
    String::from_utf8_lossy(&vm.take_output()).into_owned()
}

/// A method whose `case` alone is far over the 32 KiB a 16-bit jump reaches, with every
/// kind of jump around and inside it: loops with `next`/`break`, `&&`/`||`, modifiers,
/// blocks, `rescue`/`ensure` and safe navigation.
fn big_method(whens: usize) -> String {
    let mut s = String::from(
        "def big(x, list)\n  acc = 0\n  i = 0\n  while i < 6\n    i += 1\n    next if i == 2\n    break if i == 5\n    acc += i\n  end\n  case x\n",
    );
    for k in 0..whens {
        s += &format!("  when {k}\n    y = {k} * 2\n    y += 1 if y > 10 && x != 3\n    y -= 1 unless y.nil? || false\n    acc += y\n");
    }
    s += "  else\n    acc = -1\n  end\n  list.each { |v| next if v == 2; break if v == 9; acc += v }\n";
    s += "  begin\n    raise \"boom\" if x == 7\n  rescue => e\n    acc += 1000\n  ensure\n    acc += 1\n  end\n";
    s += "  acc += (list.first&.succ || 0)\n  acc\nend\n";
    s
}

#[test]
fn methods_too_big_for_16_bit_jumps_compile_with_long_jumps_and_run_alike() {
    let mut src = big_method(1500);
    src += "p [0, 7, 3, 1499, 2000].map { |x| big(x, [1, 2, 3, 9, 4]) }\n";
    // worked by hand: the loop leaves acc = 8, the each adds 4, ensure 1, first&.succ 2
    assert_eq!(run(&src), "[14, 1029, 20, 3013, 6]\n");
    // the same method with few `when`s compiles with ordinary jumps and agrees
    let mut small = big_method(10);
    small += "p [0, 7, 3].map { |x| big(x, [1, 2, 3, 9, 4]) }\n";
    assert_eq!(run(&small), "[14, 1029, 20]\n");
}

#[test]
fn array_sends_answered_in_place_still_follow_redefinitions() {
    let out = run(r#"
a = [10, 20, 30]
p a.length, a.size, a[1], a[-1], a[5], a[-9]
r = []; a.each { |v| r << v }; p r
class Sub < Array; end
s = Sub.new([1, 2]); p s.length, s[0]
class Array
  def length; 99; end
end
p a.length, a.size
class Sub
  def [](i); "sub#{i}"; end
end
p s[0], s.size
"#);
    assert_eq!(out, "3\n3\n20\n30\nnil\nnil\n[10, 20, 30]\n2\n1\n99\n3\n\"sub0\"\n2\n");
}

#[test]
fn cached_constants_follow_changes_to_the_tables_they_came_from() {
    let out = run(r#"
module Outer
  X = 1
  class Inner
    def read; X; end
  end
end
o = Outer::Inner.new
p o.read
Outer.send(:remove_const, :X)
Outer.const_set(:X, 2)
p o.read
class Outer::Inner
  X = 3
end
p o.read
GC.start
p o.read
"#);
    assert_eq!(out, "1\n2\n3\n3\n");
}
