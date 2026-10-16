# A Float that stands for no Integer (NaN, an infinity, a value past `mrb_int`) as an Array
# index or a `rand` limit. mruby 4.1.0-rc2 narrows it with the C cast `(mrb_int)f`
# (`aget_index` in src/array.c, `random_rand_impl` in mrbgems/mruby-random/src/random.c),
# which is undefined behaviour: on x86-64 the cast gives the smallest Integer, so rc2 answers
# nil for `a[Float::NAN]`, "index -9223372036854775808 out of array" for `a[Float::NAN] = 1`
# and some Integer for `rand(Float::NAN)` (AArch64 reads a NaN as 0, says e2148c1b1). mruby master e2148c1b1
# fixed it as a bug with the checked conversion (`mrb_as_int`, `mrb_ensure_integer_type`):
# RangeError "float NaN out of range", "float Infinity out of range", "integer out of range".
# expected-from: mruby master e2148c1b1 (image kishima/mruby:891d1d9d). CRuby raises RangeError
# for the index too ("float NaN out of range of integer") and FloatDomainError for `rand`.
# The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue Exception => e
  puts "#{label} !! #{e.class}: #{e.message}"
end
a = %w[a b c]
nan = Float::NAN; inf = Float::INFINITY
t("a[nan]") { a[nan] }
t("a[inf]") { a[inf] }
t("a[-inf]") { a[-inf] }
t("a[1e30]") { a[1e30] }
t("a[inf, 1]") { a[inf, 1] }
t("a[0, inf]") { a[0, inf] }
t("a.slice(nan)") { a.slice(nan) }
t("a[nan] = 1") { b = a.dup; b[nan] = 1; b }
t("a[1e30] = 1") { b = a.dup; b[1e30] = 1; b }
t("a[0, nan] = 1") { b = a.dup; b[0, nan] = 1; b }
t("a unchanged") { a }
# a Float that does stand for an Integer is truncated toward zero, as before
t("a[1.9]") { a[1.9] }
t("a[-1.9]") { a[-1.9] }
t("a[1.5] = 9") { b = a.dup; b[1.5] = 9; b }
t("rand(nan)") { rand(nan).class }
t("rand(inf)") { rand(inf).class }
t("rand(-inf)") { rand(-inf).class }
t("Random#rand(inf)") { Random.new(1).rand(inf).class }
t("Random.rand(nan)") { Random.rand(nan).class }
t("rand(-1e30)") { rand(-1e30).class }
t("rand(1e30)") { rand(1e30).class }
t("rand(5.9)") { srand(234); (1..20).map { rand(5.9) }.all? { |v| v.is_a?(Integer) && v >= 0 && v < 5 } }
t("rand(0.5)") { rand(0.5).class }
