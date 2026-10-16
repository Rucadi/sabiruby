# `remove_instance_variable` checks the name before anything else, as `instance_variable_get`,
# `instance_variable_set` and `instance_variable_defined?` do (docs/plans/rc2-leftovers5-plan.md,
# R2). Expectation: mruby 4.1.0-rc2 (.rc.out). `mrb_obj_remove_instance_variable` (src/kernel.c)
# calls `mrb_iv_name_sym_check` (src/variable.c) first: NameError "'x' is not allowed as an
# instance variable name" with `name` set, then `mrb_iv_remove` (the type and frozen checks),
# then NameError "instance variable @x not defined", also with `name`.
o = Object.new
o.instance_variable_set(:@a, 1)
[:x, :"@", :"@1", :"@@a", :"@a?", :@a, "@a"].each do |n|
  [:instance_variable_get, :instance_variable_defined?, :remove_instance_variable].each do |m|
    begin
      r = o.send(m, n)
      puts "#{m} #{n.inspect}: #{r.inspect}"
    rescue => e
      puts "#{m} #{n.inspect}: #{e.class}: #{e.message} name=#{e.name.inspect}"
    end
  end
  begin
    puts "set #{n.inspect}: #{o.instance_variable_set(n, 2).inspect}"
  rescue => e
    puts "set #{n.inspect}: #{e.class}: #{e.message}"
  end
end
begin; o.remove_instance_variable(:@zz); rescue => e; puts "#{e.class}: #{e.message} name=#{e.name.inspect}"; end
begin; "s".remove_instance_variable(:x); rescue => e; puts "#{e.class}: #{e.message}"; end
begin; "s".remove_instance_variable(:@x); rescue => e; puts "#{e.class}: #{e.message}"; end
begin; 1.remove_instance_variable(:x); rescue => e; puts "#{e.class}: #{e.message}"; end
begin; Object.new.freeze.remove_instance_variable(:x); rescue => e; puts "#{e.class}: #{e.message}"; end
begin; Object.new.remove_instance_variable(3); rescue => e; puts "#{e.class}: #{e.message}"; end
p Object.new.method(:instance_variable_get).owner
p Object.new.method(:remove_instance_variable).owner
