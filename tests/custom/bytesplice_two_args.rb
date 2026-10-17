# `String#bytesplice` with two arguments of which the first is no Range (rc2-sweep cause `crash`,
# docs/verification/rc2-sweep/README.md): SabiRuby stopped the process with a Rust panic (an index
# out of bounds in src/builtins/string.rs). rc2's `mrb_str_bytesplice` (src/string.c) reads two
# arguments as "oS": the second becomes a String first (TypeError), then a first argument that is
# no Range refuses the call as "wrong number of arumgnts" (rc2's spelling).
# expected-from: the reference (.expected is .rc.out). The other forms follow rc2 in
# docs/plans/rc2-sweep-fixes-plan.md Y2.
def t
  s = "abc"
  r = yield(s)
  p [r, s]
rescue Exception => e
  p [e.class, e.message]
end
t { |s| s.bytesplice(0, 1) }
t { |s| s.bytesplice(1, 0) }
t { |s| "".bytesplice(0, 0) }
t { |s| s.bytesplice(-1, nil) }
t { |s| s.bytesplice(0, "x") }
t { |s| s.bytesplice(nil, "x") }
t { |s| s.bytesplice(0..1, "x") }
t { |s| s.bytesplice(0, 1, "x") }
