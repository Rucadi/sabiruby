# Writes "Owner#name arity" for every method a named class or module defines. Run on both VMs
# by tools/rc2_aspec.sh --compare (the measurement of rc2-leftovers9).
ObjectSpace.each_object(Module) do |c|
  next if c.inspect.start_with?("#")
  (c.public_instance_methods(false) + c.private_instance_methods(false)).each do |m|
    um = c.instance_method(m) rescue next
    puts "#{c}##{m} #{um.arity}"
  end
end
