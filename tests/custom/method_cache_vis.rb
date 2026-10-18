# The visibility a call with a written receiver checks, after the answer has been made
# cacheable by the same call a few hundred times (rc2-sweep-fixes Y8a: the method cache keeps
# the visibility beside the method, so a hit does not look it up again). Each change below
# must reach the next call: prepend, `private`/`public` back and forth, `module_function`,
# `protected`, and `private` on an inherited method (which copies it into the class first,
# `Vm::set_visibility`). Expectation: the reference mruby's own output.
def call(n = 200)
  r = nil
  n.times { r = begin; yield; rescue NoMethodError => e; e.message.split(" for ").first; end }
  r
end

class A
  def m; "A#m"; end
  def n; "A#n"; end
end
class B < A; end
a = A.new; b = B.new

# 1. private and back, on the owner
puts call { a.m }                        # A#m
class A; private :m; end
puts call { a.m }                        # private method 'm' called
class A; public :m; end
puts call { a.m }                        # A#m
class A; protected :m; end
puts call { a.m }                        # protected method 'm' called
class A; public :m; end
puts call { a.m }

# 2. private on a subclass copies the inherited method there; the superclass stays public
puts call { b.n }
class B; private :n; end
puts call { b.n }                        # private
puts call { a.n }                        # A#n
class B; public :n; end
puts call { b.n }
class B; remove_method :n; end
class A; private :n; end
puts call { b.n }                        # private, found in A
class A; public :n; end
puts call { b.n }

# 3. a private method of a prepended module, and the class's own one under it
module P
  def m; "P#m+" + super; end
end
class C; def m; "C#m"; end; end
c = C.new
puts call { c.m }
class C; prepend P; end
puts call { c.m }                        # P#m+C#m
module P; private :m; end
puts call { c.m }                        # private (P's m answers first)
module P; public :m; end
puts call { c.m }
module P; remove_method :m; end
puts call { c.m }                        # C#m, from the origin
class C; private :m; end
puts call { c.m }                        # private: C's own m, in the origin
class C; public :m; end
puts call { c.m }

# 4. module_function makes the instance method private and copies it to the singleton
module F
  def f; "F#f"; end
end
class G; include F; end
g = G.new
puts call { g.f }
module F; module_function :f; end
puts call { g.f }                        # private
puts call { F.f }                        # F#f
module F; public :f; end
puts call { g.f }

# 5. protected: callable from an instance of the owner, not from outside
class H
  def p1; "H#p1"; end
  def via(o); o.p1; end
end
class I < H; end
h = H.new; i = I.new
puts call { i.p1 }
class H; protected :p1; end
puts call { i.p1 }                       # protected
puts call { h.via(i) }                   # H#p1
puts call { i.via(h) }                   # H#p1
puts call { Object.new.instance_eval { h.p1 } }  # protected
class I; public :p1; end
puts call { i.p1 }                       # I#p1 copy is public
puts call { h.p1 }                       # still protected on H

# 6. a singleton method made private on one object
o1 = A.new; o2 = A.new
def o1.s; "o1.s"; end
puts call { o1.s }
o1.singleton_class.send(:private, :s)
puts call { o1.s }
puts call { o2.m }
