# A bare `raise` re-raises what `$!` names, which code compiled by mruby 891d1d9d's `mrbc` sets in
# a rescue clause, a rescue modifier and an ensure an exception is unwinding (`OP_SETGV $!`);
# outside one it is a RuntimeError with an empty message (mruby 891d1d9d `mrb_f_raise`;
# docs/plans/edge-diff-fix-plan.md D2). The case is compiled here by rc2's `mrbc`
# (tools/custom.sh), which does not set `$!`, so it sets the name itself the way that code
# does. Expectation: the output of mruby 891d1d9d (Docker kishima/mruby:891d1d9d); rc2's
# `raise` never re-raises (.rc.out).
def reraise; raise; end
begin
  $! = TypeError.new("inner")
  raise
rescue => e
  p [e.class, e.message]
end
begin
  $! = ArgumentError.new("through a method")
  reraise
rescue => e
  p [e.class, e.message]
end
$! = nil
begin
  raise
rescue => e
  p [e.class, e.message]
end
