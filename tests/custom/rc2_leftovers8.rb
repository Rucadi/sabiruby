# The three differences from mruby 4.1.0-rc2 found in rc2-leftovers7 (docs/plans/
# rc2-leftovers8-plan.md, U1). Expectation: mruby 4.1.0-rc2 (.rc.out).
# `mrb_init_class` (src/class.c) undefines `module_function` in Class. `str_hex`/`str_oct`
# (mruby-string-ext) are `mrb_str_to_integer(self, 16 / 8, FALSE)`, the scanner of
# `String#to_i`. The Float and Integer methods whose entry is `MRB_ARGS_NONE` (src/numeric.c,
# mruby-numeric-ext, mruby-rational) refuse an argument.
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end

# Class#module_function
c = Class.new
t("Class.new { module_function }") { Class.new { module_function } }
t("class body module_function") { c.class_eval { module_function :to_s } }
t("send(:module_function)") { c.send(:module_function) }
t("Class responds") { c.respond_to?(:module_function, true) }
t("Module responds") { Module.new.respond_to?(:module_function, true) }
t("Class private ims") { Class.private_instance_methods(false).include?(:module_function) }
t("Module private ims") { Module.private_instance_methods(false).include?(:module_function) }
m = Module.new { def y; 1; end; module_function :y }
t("module_function in a module") { m.y }

# String#hex / String#oct
["ff", "0xff", "0Xff", "-0xff", "+ff", " ff", "\t-ff", "f_f", "f__f", "_ff", "ff_", "0x",
 "0x_f", "0_x", "", "-", "z", "0b11", "0o17", "17", "017", "0o", "-017", "8", "1_7", "1__7",
 "0_17", "00_17", "+_ff", "-_17", "12abc", "0d12", "0O7", "0__0x1", "ff\0ff",
 "7fffffffffffffff", "8000000000000000", "-8000000000000000", "-7fffffffffffffff",
 "ffffffffffffffffffff", "-ffffffffffffffffffff", "777777777777777777777",
 "1000000000000000000000", "-1000000000000000000000", "7777777777777777777777777"].each do |s|
  t("#{s.inspect}.hex / .oct") { [s.hex, s.oct] }
end
t("hex(1)") { "ff".hex(1) }
t("oct(1)") { "17".oct(1) }

# MRB_ARGS_NONE
[[1.5, %i[-@ abs finite? infinite? nan? to_f to_i to_r to_s]],
 [7, %i[-@ ~ bit_length even? odd? finite? infinite? size to_f to_i to_int to_r]]].each do |v, ms|
  ms.each do |m|
    # `num_infinite_p` answers false for an Integer where `flo_infinite_p` answers nil; SabiRuby
    # answers nil for both (left as it is: docs/worklog/2026-10-17-rc2-leftovers8.md)
    t("#{v}.#{m}") { v.__send__(m) } unless v.is_a?(Integer) && m == :infinite?
    t("#{v}.#{m}(2)") { v.__send__(m, 2) }
  end
end
t("(2**70).size(1)") { (2**70).size(1) }
t("Float::INFINITY.to_i(1)") { Float::INFINITY.to_i(1) }
