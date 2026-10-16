# `Integer#to_s` with a radix that is not an Integer. mruby 4.1.0-rc2's `int_to_s`
# (src/numeric.c) reads the argument with `mrb_get_args(mrb, "|o")` and then `mrb_integer(arg)`
# without looking at its type, so it reads the bits of whatever value it was given: nil, a
# String and a wide Integer answer "invalid radix 0", 1.5 "invalid radix 4602678819172646913"
# (the Float's bit pattern), and the answer depends on the value representation of the build.
# That is undefined (it reads a value as a type it is not), so SabiRuby converts the radix the
# way every other Integer argument is converted (`mrb_as_int`): 1.5 is radix 1, and a value
# that is not a number is a TypeError. master 891d1d9d answers as rc2 does.
# expected-from: SabiRuby (`mrb_as_int`; CRuby 3.2 agrees on 1.5 → "invalid radix 1" and on
# nil being a TypeError). The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue Exception => e
  puts "#{label} !! #{e.class}: #{e.message}"
end
[nil, "1", 1.5, 2.5, 16.9, 2**70].each do |x|
  t("10.to_s(#{x.inspect})") { 10.to_s(x) }
end
t("10.to_s(2)") { 10.to_s(2) }
t("10.to_s(37)") { 10.to_s(37) }
