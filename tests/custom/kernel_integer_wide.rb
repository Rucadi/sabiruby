# Integer() and Float() on the numeric types besides Integer and Float (docs/plans/
# rc2-ivar-plan.md, Q2). Expectation: mruby 4.1.0-rc2 (.rc.out). `mrb_f_integer`
# (mrbgems/mruby-kernel-ext/src/kernel.c) hands a Float to `mrb_float_to_integer` (a wide
# Integer past mrb_int, RangeError "float NaN out of range") and anything not a String to
# `mrb_ensure_integer_type` (src/object.c): a wide Integer as it is, a Rational or a Complex
# through `mrb_rational_to_i` / `mrb_complex_to_i`, else TypeError "%Y cannot be converted to
# Integer"; with a base, `mrb_obj_as_string` first. `mrb_f_float` uses `mrb_ensure_float_type`
# the same way. `mrb_complex_to_i` / `to_f` name the value with `%v` (to_s, no parentheses).
def t(label)
  r = yield
  puts "#{label} => #{r.inspect}"
rescue => e
  puts "#{label} => #{e.class}: #{e.message}"
end
t("Integer(2**70)") { Integer(2**70) }
t("Integer(-2**70)") { Integer(-(2**70)) }
t("Integer(2**70, 16)") { Integer(2**70, 16) }
t("Integer(1e30)") { Integer(1e30) }
t("Integer(-1e19)") { Integer(-1e19) }
t("Integer(1.5)") { Integer(1.5) }
t("Integer(3r/2)") { Integer(3r/2) }
t("Integer(2**70*1r)") { Integer((2**70).to_r) }
t("Integer(2+0i)") { Integer(2+0i) }
t("Integer(2+1i)") { Integer(2+1i) }
t("Integer(Object.new)") { Integer(Object.new) }
t("Integer(:a)") { Integer(:a) }
t("Integer([])") { Integer([]) }
t("Integer(true)") { Integer(true) }
t("Integer(1r, 10)") { Integer(1r, 10) }
t("Float(2**70)") { Float(2**70) }
t("Float(-2**70)") { Float(-(2**70)) }
t("Float(3r/2)") { Float(3r/2) }
t("Float(2+0i)") { Float(2+0i) }
t("Float(2+1i)") { Float(2+1i) }
t("Float(nil)") { Float(nil) }
t("Float(Object.new)") { Float(Object.new) }
t("Float(:a)") { Float(:a) }
t("Rational(2**70)") { Rational(2**70) }
t("Rational(2**70, 3)") { Rational(2**70, 3) }
t("Complex(2**70)") { Complex(2**70) }
t("Complex(2**70, 2**70)") { Complex(2**70, 2**70) }
t("NaN") { Integer(Float::NAN) }
t("Inf") { Integer(-Float::INFINITY) }
t("NaN.to_i") { Float::NAN.to_i }
t("Rational(2**70, 2**69)") { Integer(Rational(2**70, 2**69)) }
t("Complex(1.5, 0)") { Integer(Complex(1.5, 0)) }
t("Complex(1e30, 0)") { Integer(Complex(1e30, 0)) }
t("Complex(3r/2, 0)") { Integer(Complex(3r/2, 0)) }
t("Float Complex(3r/2, 0)") { Float(Complex(3r/2, 0)) }
t("Float(Complex(2**70, 0))") { Float(Complex(2**70, 0)) }
t("Integer('1', 2**70)") { Integer("1", 2**70) }
