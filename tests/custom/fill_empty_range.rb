# `Array#fill` with a Range the array cannot take (rc2-sweep cause `fill-empty-range`): rc2's
# `ary_fill_parse_arg` (mruby-array-ext) tests `mrb_range_beg_len(...)` as a boolean, so
# MRB_RANGE_OUT (nonzero) passes and the start and length it never set are read —
# `[].fill(0, 1..3)` gave `[nil, nil, nil, 0, 0, 0, 0, 0, 0, 0]` or "array size too big",
# different from run to run. The author took CRuby's answer (2026-10-18; plan Y2, the record
# is Y5): a start past the end fills from there, a start before the array is RangeError.
# Ranges the array can take keep rc2's answer (its end is truncated, `[1, 2, 3].fill(0, 1..5)`
# is `[1, 0, 0]`, where CRuby extends the array).
# expected-from: CRuby 3.2 for the out-of-range cases, the reference for the others; .rc.out
# is rc2's, which reads uninitialised memory.
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

t("empty 1..3") { [].fill(0, 1..3) }
t("empty 1..") { [].fill(0, 1..) }
t("past end") { [1, 2].fill(0, 5..6) }
t("past end excl") { [1, 2].fill(0, 5...6) }
t("before") { [].fill(0, -5..-1) }
t("before 2") { [].fill(0, -1..2) }
t("block") { [].fill(1..3) { |i| i } }
t("reversed") { [1].fill(0, 3..1) }
t("in range") { [1, 2, 3].fill(9, 0...2) }
t("in range end past") { [1, 2, 3].fill(0, 1..5) }
t("in range endless") { [1, 2, 3].fill(9, -2..) }
