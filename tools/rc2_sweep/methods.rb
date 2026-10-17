# Every class and module reachable from Object (and one level of nesting), with the methods
# each defines itself: instance methods by visibility and singleton methods. Run on both VMs
# by tools/rc2_sweep/sweep.py; one line per method: kind TAB owner TAB name.
seen = {}
mods = []
Object.constants.sort.each do |c|
  v = Object.const_get(c) rescue next
  next unless v.is_a?(Module)
  next if seen[v.__id__] || v.name != c.to_s   # an alias such as Fixnum = Integer
  seen[v.__id__] = true
  mods << [c.to_s, v]
end
mods.dup.each do |name, m|
  (m.constants rescue []).sort.each do |c|
    v = m.const_get(c) rescue next
    next unless v.is_a?(Module)
    next if seen[v.__id__]
    seen[v.__id__] = true
    mods << ["#{name}::#{c}", v]
  end
end
mods.each do |name, m|
  puts "M\t#{name}\t#{m.class}"
  m.public_instance_methods(false).sort.each { |x| puts "public\t#{name}\t#{x}" }
  m.protected_instance_methods(false).sort.each { |x| puts "protected\t#{name}\t#{x}" }
  m.private_instance_methods(false).sort.each { |x| puts "private\t#{name}\t#{x}" }
  m.singleton_methods(false).sort.each { |x| puts "singleton\t#{name}\t#{x}" }
end
