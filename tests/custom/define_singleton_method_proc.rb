# `define_singleton_method(name, a_proc)`: the body as the second argument. mruby 4.1.0-rc2
# declares the method `MRB_ARGS_REQ(1)|MRB_ARGS_BLOCK()` (mruby-metaprog metaprog.c:701; master
# 6db46e7c4 the same at :743), so the VM refuses a second argument before the body runs, while
# the body reads it ("n|o&", `define_method_m`, src/class.c) and `Module#define_method`, the same
# body, declares two. That is inconsistent within mruby itself, so SabiRuby declares it as
# `define_method` is and takes the Proc (the author's decision of 2026-10-18).
# expected-from: CRuby 3.2 (the method is defined). The reference's ArgumentError is in .rc.out.
# README, "Known deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end
o = Object.new
t("define_singleton_method(:x, proc)") { o.define_singleton_method(:x, proc { |a| a * 2 }) }
t("o.x(3)") { o.x(3) }
t("o.x(1, 2)") { o.x(1, 2) }
t("define_singleton_method(:y) { }") { o.define_singleton_method(:y) { 1 } }
t("o.y") { o.y }
t("define_singleton_method(:z, 1)") { o.define_singleton_method(:z, 1) }
t("define_singleton_method(:z, proc {}, 3)") { o.define_singleton_method(:z, proc {}, 3) }
t("define_singleton_method arity") { Object.instance_method(:define_singleton_method).arity }
