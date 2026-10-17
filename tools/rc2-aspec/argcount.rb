# Calls every method with one argument too many (arity >= 0) and one too few (required > 0)
# and prints what was raised. Run on both VMs by tools/rc2_aspec.sh --compare. Methods that
# would block, exit, print or change shared state are left out (SKIP).
SKIP = %i[exit exit! abort loop sleep usleep sleep_ms fork system exec ` gets readline readlines open
  binding yield raise fail throw catch eval instance_eval class_eval module_eval instance_exec class_exec module_exec
  require load require_relative __exec_file __load_file each_object garbage_collect start pass run tick join
  resume transfer alive? suspend terminate printf print puts p putc __printstr__ display srand trap kill
  define_method define_singleton_method remove_method undef_method alias_method remove_const const_set
  instance_variable_set class_variable_set attr attr_reader attr_writer attr_accessor include prepend extend
  extend_object append_features prepend_features module_function public private protected private_constant
  method_missing initialize initialize_copy freeze replace clear __send__ send public_send
  shift pop push unshift concat insert delete delete_at delete_if keep_if select! reject! map! collect!
  fill sort! reverse! shuffle! uniq! compact! flatten! rotate! slice! []= store update merge! rehash
  upcase! downcase! capitalize! swapcase! chomp! chop! strip! lstrip! rstrip! squeeze! tr! tr_s! sub! gsub!
  succ! next! setbyte force_encoding << prepend scheduler_driven= debt_limit= add add? delete? subtract
  ]
def sample(c)
  case c.name
  when "Integer" then 1
  when "Float" then 1.5
  when "String" then "ab"
  when "Symbol" then :s
  when "Array" then [1]
  when "Hash" then {a: 1}
  when "Range" then (1..2)
  when "NilClass" then nil
  when "TrueClass" then true
  when "FalseClass" then false
  when "Proc" then proc { }
  when "Object", "BasicObject", "Kernel" then Object.new
  when "Comparable" then 1
  when "Enumerable" then [1]
  when "Module" then Module.new
  when "Class" then Class.new
  when "Method" then 1.method(:+)
  when "UnboundMethod" then Integer.instance_method(:+)
  when "Rational" then Rational(1, 2)
  when "Complex" then Complex(1, 2)
  when "Set" then Set[1]
  when "Time" then Time.at(0)
  when "Regexp" then /a/
  when "MatchData" then "a".match(/a/)
  when "Random" then Random.new(1)
  when "Numeric" then 1
  else
    if c.is_a?(Class) && c <= Exception then (c.new("x") rescue nil) else nil end
  end
end
def probe(label, recv, m, n)
  args = Array.new(n) { |i| i }
  r = begin
    recv.__send__(m, *args)
    "returned"
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} #{n} => #{r}"
end
mods = []; ObjectSpace.each_object(Module) { |c| mods << c }
mods.sort_by { |c| c.inspect }.each do |c|
  next if c.inspect.start_with?("#")
  [[c, "#", nil], [c.singleton_class, ".", c]].each do |k, sep, rcv|
    names = (k.public_instance_methods(false) + k.protected_instance_methods(false) + k.private_instance_methods(false)).uniq.sort
    names.each do |m|
      next if SKIP.include?(m) || m == :__aspec
      recv = rcv || sample(c)
      next if recv.nil? && c != NilClass
      next if sep == "#" && !recv.is_a?(c)
      a = (k.instance_method(m).arity rescue next)
      label = "#{c}#{sep}#{m}"
      if a >= 0
        probe(label, recv, m, a + 1)
        probe(label, recv, m, a - 1) if a >= 1
      elsif a < -1
        probe(label, recv, m, -a - 2)
      end
    end
  end
end
