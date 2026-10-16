# Reading and writing instance variables of a plain object: `@x` and `@x = v` (GETIV, SETIV)
# and an `attr_accessor` pair. The two instructions ask whether the receiver can hold instance
# variables (`obj_iv_p`) since rc2-ivar. Same loop and count as m_loop_only.rb.
class Holder
  attr_accessor :b
  def initialize; @a = 0; @b = 0; end
  def run
    i = 0
    while i < 200_000
      @a = @a + 1
      @a = @a + 1
      self.b = b + 1
      i += 1
    end
    @a
  end
end
Holder.new.run
