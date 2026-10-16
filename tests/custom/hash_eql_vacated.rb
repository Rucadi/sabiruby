# A Hash entry taken away by the Ruby a lookup or a scan calls: the key's `eql?` (or `==`,
# `inspect`, `hash`) deletes the very entry being looked at and stores another, which puts the
# size back. A deviation from mruby 4.1.0-rc2 taken under docs/design/compatibility.md ("mruby
# itself fixed it later as a bug"; README.md, "Known deviations"). rc2's src/hash.c (`ar_get`,
# `ib_it_find_by_key`, `ht_set`, `ea_get_by_key`, `mrb_hash_assoc`, `mrb_hash_rassoc`,
# `mrb_hash_equal`, `mrb_hash_eql`, `mrb_hash_to_s`, `mrb_hash_except_keys`, `ar_rehash`,
# `ht_rehash`) reads the vacated slot afterwards, a read of freed memory once its value is
# collected; only an AR Hash with no hole ahead of the entry raises, because its
# H_CHECK_MODIFIED reads the first slot.
# expected-from: mruby master a54353ecf, 08a0432d1, eb7693857 (`entry_key_eql`,
# `entry_check_vacated`; output of master 891d1d9d and 158760853): RuntimeError "hash modified"
# everywhere, the Hash left consistent.
# rc2 answers (.rc.out): HT and AR with a hole: delete 9 (1) with the size one short (in HT
# `:added` is then found by `key?` but not listed by `keys`), [] 9 (1), []= stores nothing,
# key? true;
# AR with no hole raises; assoc/rassoc a pair, == and eql? true, inspect a string, __except a
# NoMethodError (`hash` sent to the vacated key), rehash drops an entry.
# CRuby 3.2.6 and 3.4 compare hash codes before `eql?`, so the first part never calls the
# callback (nil, false, a new entry); the scans raise "can't add a new key into hash during
# iteration" or answer nil.
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue => e
  puts "#{label}: #{e.class}: #{e.message}"
end
def consistent(h) h.keys.all? { |k| h.key?(k) } && h.key?(:added) == h.keys.include?(:added) end

swapper = Class.new do
  def initialize(h) @h, @fired = h, false end
  def eql?(other)
    unless @fired
      @fired = true
      @h.delete(other)
      @h[:added] = :added_value
    end
    true
  end
  def hash; 42 end
end
ht = lambda { h = {}; 20.times { |i| h[i] = i }; h }
ar_hole = lambda { h = {}; 12.times { |i| h[i] = i }; h.delete(0); h }
ar = lambda { h = {}; 12.times { |i| h[i] = i }; h }
[[:ht, ht], [:ar_hole, ar_hole], [:ar, ar]].each do |name, build|
  [[:delete, lambda { |h, k| h.delete(k) }],
   [:aref, lambda { |h, k| h[k] }],
   [:aset, lambda { |h, k| h[k] = :stored }],
   [:key?, lambda { |h, k| h.key?(k) }]].each do |op, f|
    h = build.call
    t("#{name} #{op}") { f.call(h, swapper.new(h)) }
    puts "  size #{h.size}, consistent #{consistent(h)}"
  end
end

# the scans that read the entry after a comparison
armed = Class.new do
  attr_accessor :armed
  def initialize(h, key) @h, @key, @armed = h, key, false end
  def eql?(other)
    if @armed
      @armed = false
      @h.delete(@key || self)
      @h[:added] = :added_value
    end
    other.class == self.class
  end
  def hash; 42 end
end
h = {}; 20.times { |i| h[i] = i }; k = armed.new(h, nil); h[k] = "value"; k.armed = true
t("assoc") { h.assoc(armed.new(h, nil)).size }
puts "  consistent #{consistent(h)}"
h = {}; 20.times { |i| h[i] = i }; v = armed.new(h, :k); h[:k] = v; v.armed = true
t("rassoc") { h.rassoc(armed.new(h, nil)).size }
puts "  consistent #{consistent(h)}"
[:==, :eql?].each do |op|
  h1 = {}; 20.times { |i| h1[i] = i }; k = armed.new(h1, nil); h1[k] = "value"
  h2 = {}; 20.times { |i| h2[i] = i }; h2[armed.new(h2, nil)] = "value"
  k.armed = true
  t(op) { h1.__send__(op, h2) }
  puts "  consistent #{consistent(h1)}"
end

# the scans that read the entry after any other callback
firing = Class.new do
  attr_accessor :armed
  def initialize(h, victim) @h, @victim, @armed = h, victim, false end
  def fire
    return false unless @armed
    @armed = false
    @h.delete(@victim || self)
    @h[:added] = :added_value
    true
  end
  def inspect; fire; "firing" end
  def ==(other) fire; false end
  def eql?(other)
    return true if (@victim.nil? || @victim.equal?(other)) && fire
    equal?(other)
  end
  def hash; 42 end
end
h = {}; 20.times { |i| h[i] = i }; k = firing.new(h, nil); h[k] = "value"; k.armed = true
t("inspect") { h.inspect.size }
puts "  consistent #{consistent(h)}"
h = {}; 20.times { |i| h[i] = i }; k = firing.new(h, nil); h[k] = "value"; k.armed = true
t("__except") { h.__except([:absent]).size }
puts "  consistent #{consistent(h)}"
h = {first: "value"}; k = firing.new(h, nil); h[k] = "dup"; k.armed = true
t("rehash ar") { h.rehash.size }
puts "  size #{h.size}, consistent #{consistent(h)}"
h = {}; 20.times { |i| h[i] = i }; first = firing.new(h, nil); h[first] = "value"
k = firing.new(h, first); h[k] = "dup"; k.armed = true
t("rehash ht") { h.rehash.size }
puts "  added #{h.key?(:added)}"
