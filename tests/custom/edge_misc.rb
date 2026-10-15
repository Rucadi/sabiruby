# Four small differences from mruby 891d1d9d (docs/plans/edge-diff-fix-plan.md D6): a modulo by
# a zero Float raises (`flodivmod`, and `int_mod` without its early answer for 0); a Float
# index no Integer holds is out of range (`aget_index` takes `mrb_as_int`); an interpolated
# object whose `to_s` answers no String is spelled by the default representation
# (`prepare_exec_strcat_post_func`); the GC ratios refuse a value past a C int. Expectation: the
# output of mruby 891d1d9d (Docker kishima/mruby:891d1d9d).
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
