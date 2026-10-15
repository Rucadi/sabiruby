# A bare `raise` is a RuntimeError with an empty message, whatever `$!` names (mruby 4.1.0-rc2
# `mrb_f_raise`). The case sets `$!` itself, the way code compiled by mruby 891d1d9d's `mrbc`
# does in a rescue clause (`OP_SETGV $!`); 891d1d9d's `raise` then re-raises it (c05a266a5,
# after the stable branch point). Expectation: mruby 4.1.0-rc2 (.rc.out), which SabiRuby keeps
# (docs/worklog/2026-10-16-follow-master.md).
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
