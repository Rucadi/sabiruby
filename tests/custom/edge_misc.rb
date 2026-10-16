# Modulo, a Float index, interpolation and the GC ratios as mruby 4.1.0-rc2 answers them: a zero
# receiver of `Integer#%` answers 0 before the divisor is looked at, a zero Float divisor raises
# otherwise (`flodivmod`); a Float index is narrowed by the C cast, which gives the smallest
# Integer for a NaN or a value out of range on x86-64; interpolation spells a String, a Symbol,
# a fixnum and a class itself and sends `to_s` to anything else, an answer that is no String
# giving way to the default representation (`mrb_obj_as_string`); the GC ratios take any
# Integer. mruby 891d1d9d raises for `0 % 0` (80747a5f6), raises RangeError for such an index
# (e2148c1b1), stops sending `to_s` to Float, nil, true and false (ee3cdfdf7) and refuses a
# ratio past a C int (e2b4585e4), all after the stable branch point. Expectation: mruby
# 4.1.0-rc2 (.rc.out), the IndexError's message included (`mrb_ary_set`'s "out of array" since
# docs/worklog/2026-10-16-rc2-leftovers.md). docs/worklog/2026-10-16-follow-master.md.
def try
  yield
rescue => e
  "#{e.class}: #{e.message}"
end
p [try { 0 % 0.0 }, try { 1 % 0.0 }, try { 1.5 % 0.0 }, try { 1.5 % 0 }, 0 % 2.5, try { 0 % 0 }]
a = %w[a b c]
p [try { a[Float::INFINITY] }, try { a[Float::NAN] }, try { a[1e30] }, try { a[Float::NAN, 1] }]
p [try { a[Float::NAN] = 1 }, a, a[1.9], a[-1.9]]
bad = Class.new { def to_s; 42; end }
nils = Class.new { def to_s; nil; end }
good = Class.new { def to_s; "called"; end }
p ["#{bad.new}"[0, 2], "#{nils.new}"[0, 2], "x #{good.new}", "#{1.5} #{nil} #{true} #{:s} #{1 << 70}"]
p [try { GC.step_ratio = 1 << 40 }, try { GC.interval_ratio = 1 << 40 }, try { GC.interval_ratio = -1 }]
