# Writes what the reference VM keeps for each method, in two parts.
#
# Lines with three fields: one per method a class or module defines, with its argument spec
#   Owner#name<TAB>kind<TAB>aspec      (an instance method)
#   Owner.name<TAB>kind<TAB>aspec      (a method of Owner's singleton class)
#   main.name<TAB>kind<TAB>aspec       (a method of the top-level object's singleton class)
# kind and aspec are what Kernel#__aspec (gem/src/aspec_dump.c) answers.
#
# Lines starting with "@": which of those a class's instances (or the class itself) reach
#   @Class#name<TAB>Owner#name
# so that a method defined higher or lower in the chain on one side can be matched with the
# other side's. Without Kernel#__aspec (on SabiRuby) only the "@" lines are written.
def key_of(owner, m)
  s = owner.inspect
  if s.start_with?("#<Class:") && s.end_with?(">") then "#{s[8..-2]}.#{m}" else "#{s}##{m}" end
end
lines = []
asp = respond_to?(:__aspec, true)
ObjectSpace.each_object(Module) do |c|
  next if c.inspect.start_with?("#")
  [[c, "#"], [c.singleton_class, "."]].each do |k, sep|
    if asp
      (k.public_instance_methods(false) + k.protected_instance_methods(false) + k.private_instance_methods(false)).uniq.each do |m|
        next if m == :__aspec
        r = __aspec(k, m)
        lines << "#{c}#{sep}#{m}\t#{r[0]}\t#{r[1].nil? ? '-' : r[1]}" if r
      end
    end
    (k.public_instance_methods(true) + k.protected_instance_methods(true) + k.private_instance_methods(true)).uniq.each do |m|
      next if m == :__aspec
      um = (k.instance_method(m) rescue nil)
      lines << "@#{c}#{sep}#{m}\t#{key_of(um.owner, m)}" if um
    end
  end
end
if asp
  main = self.singleton_class
  (main.public_instance_methods(false) + main.private_instance_methods(false)).uniq.each do |m|
    r = __aspec(main, m)
    lines << "main.#{m}\t#{r[0]}\t#{r[1].nil? ? '-' : r[1]}" if r
  end
end
puts lines.sort.uniq
