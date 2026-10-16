# `__method__` in a block run by `instance_exec` or `class_exec` at the top level. mruby
# 4.1.0-rc2 answers `:instance_exec` and `:class_exec` (the block runs on a frame that keeps
# the `mid` of the method that called it), while `instance_eval` and `class_eval` with a block
# answer nil: the same kind of method answers differently within mruby. master 891d1d9d is the
# same (b0b1de54e fixed only `eval`'s string). The author decided (2026-10-17,
# docs/plans/rc2-leftovers7-plan.md, T3) to keep SabiRuby's nil, which is also CRuby's;
# answering `:instance_exec` would change the shape of the block's frame.
# expected-from: SabiRuby (CRuby 3.2 agrees). The reference's answers are in .rc.out. README,
# "Known deviations".
p 1.instance_exec { __method__ }
p Object.new.instance_eval { __method__ }
p Class.new.class_exec { __method__ }
p Class.new.class_eval { __method__ }
def m; [1].instance_exec { __method__ }; end
p m
