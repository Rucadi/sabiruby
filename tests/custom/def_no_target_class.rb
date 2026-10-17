# `def`, `alias` and `undef` in a frame with no target class (rc2-sweep cause `tdef-no-target`):
# `instance_eval`/`instance_exec` on an Integer, a Float or a Symbol, which have no singleton
# class (`mrb_singleton_class_ptr` is NULL), give the block's frame no class. rc2's OP_TDEF,
# OP_ALIAS, OP_UNDEF and OP_TCLASS call `check_target_class` (src/vm.c): TypeError "no class/module
# to add method". SabiRuby defined the method on the class the block was written in (Object
# here), and `undef` took the method away from Object. A block made inside such a frame has no
# class either; constants, class variables and `class` read Object or the block's own class, as
# rc2 does with NULL. The book's program is data/code/exp_TDEF_target_class.rb (opcodes.re, TDEF).
# expected-from: the reference (.expected is .rc.out).
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end
t(:def) { 1.instance_eval { def x1; end } }
t(:alias) { 1.instance_eval { alias x2 to_s } }
t(:undef) { 1.instance_eval { undef to_s } }
t(:float) { 1.5.instance_eval { def x3; end } }
t(:symbol) { :s.instance_eval { def x4; end } }
t(:instance_exec) { 1.instance_exec { def x5; end } }
t(:string) { 1.instance_eval("def x6; end") }
t(:nested_block) { 1.instance_eval { [1].each { def x7; end } } }
t(:proc_later) { pr = 1.instance_eval { proc { def x8; end } }; pr.call }
t(:proc_in_class_eval) { pr = 1.instance_eval { proc { def x9; end } }; String.class_eval(&pr); String.method_defined?(:x9) }
t(:eval_in_block) { 1.instance_eval { eval("def x10; end") } }
t(:eval_in_nested) { 1.instance_eval { [1].each { eval("def x11; end") } } }
t(:binding_eval) { 1.instance_eval { binding.eval("def x12; end") } }
t(:singleton_def) { 1.instance_eval { def self.x13; end } }
t(:nil) { nil.instance_eval { def x14; end }; nil.x14 }
t(:defined) { o = Object.new; [:x1, :x2, :x3, :x7, :x8, :x10].map { |m| o.respond_to?(m, true) } }
t(:to_s_kept) { Object.new.respond_to?(:to_s) }
class K
  Z = :kz
  @@cv = :cv
  t(:const) { 1.instance_eval { [Z, [1].map { Z }] } }
  t(:cvar) { 1.instance_eval { [@@cv, [1].map { @@cv }] } }
  t(:setconst) { 1.instance_eval { W1 = 1; [1].each { W2 = 2 } }; [K.constants(false).include?(:W1), Object.const_defined?(:W2)] }
  t(:class) { 1.instance_eval { class C1; end; [1].each { class C2; end } }; [K.constants(false).include?(:C1), Object.const_defined?(:C2)] }
  t(:string_const) { 1.instance_eval("[Z, String]") }
  t(:string_setconst) { 1.instance_eval("W3 = 3"); [K.constants(false).include?(:W3), Object.const_defined?(:W3)] }
  t(:nesting) { 1.instance_eval { [Module.nesting, [1].map { Module.nesting }] } }
  t(:string_nesting) { 1.instance_eval("Module.nesting") }
  t(:constants) { 1.instance_eval { [1].map { Module.constants.include?(:String) } } }
end
t(:each_object) { n = 0; ObjectSpace.each_object(Module) { |m| n += 1 if m.name.nil? && !m.singleton_class? && m.instance_of?(Module) }; n }
