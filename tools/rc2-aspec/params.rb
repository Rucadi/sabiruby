# Writes "Owner#name arity parameters" for every method a named class or module defines, and
# "Owner.name ..." for those of its singleton class. Run on both VMs by
# tools/rc2_aspec.sh --compare (rc2-sweep-fixes Y6b: native-params).
ObjectSpace.each_object(Module) do |c|
  next if c.inspect.start_with?("#")
  [[c, "#"], [c.singleton_class, "."]].each do |k, sep|
    (k.public_instance_methods(false) + k.private_instance_methods(false) +
     k.protected_instance_methods(false)).uniq.each do |m|
      um = k.instance_method(m) rescue next
      pr = um.parameters rescue "raise"
      puts "#{c}#{sep}#{m} #{um.arity} #{pr.inspect}"
    end
  end
end
