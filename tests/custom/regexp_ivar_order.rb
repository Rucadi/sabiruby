# regexp-only: Regexp and MatchData
# The order of a Regexp's own instance variables (docs/plans/rc2-leftovers5-plan.md, R3).
# Expectation: mruby 4.1.0-rc2 (.rc.out). `re_initialize` (mruby-regexp/src/regexp.c) sets
# @source and then @flags, but a Regexp (CDATA) keeps them in an `iv_tbl` sorted by symbol, so
# `instance_variables` starts with @flags; @named_captures follows. A MatchData has none. Not
# here: a subclass that sets its own variable before `super` (rc2 still lists it after @source,
# being a later symbol; SabiRuby lists it in the order set).
r = /a(b)?/i
p r.instance_variables
p //.instance_variables
p Regexp.new("x", Regexp::MULTILINE).instance_variables
m = r.match("xab")
p m.instance_variables
m2 = "zz".match(/z/)
p m2.instance_variables
r.instance_variable_set(:@q, 1); p r.instance_variables
p r.dup.instance_variables
p Regexp.union(/a/, /b/).instance_variables
p r.inspect
p m.inspect
p(/(?<a>x)(?<b>y)/.instance_variables)
r3 = /(?<a>x)/; r3.instance_variable_set(:@zz, 1); p r3.instance_variables
p(/(?<a>x)/.dup.instance_variables)
