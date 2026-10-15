# Which entries a Hash lookup compares a key with, in what order, and when it asks the key's
# `hash`: mruby 4.1.0-rc2's src/hash.c, which SabiRuby follows (src/hashtable.rs,
# docs/worklog/2026-10-16-follow-master.md M1). Up to 16 entries every entry is compared and no
# `hash` is asked; past that the key's code picks a bucket and every live entry on the probe is
# compared, whatever its own code. Symbol keys are left out: SabiRuby's symbol numbers are not
# the reference's, so their buckets differ. Expectation: mruby 4.1.0-rc2 (.rc.out).
$calls = []
K = Class.new { def eql?(o) $calls << o; false end; def hash; $calls << :hash; 42 end }
def calls; r = $calls; $calls = []; r end
h = {"a" => 1, "b" => 2, 3 => 3}; h[K.new]; p calls
t = {}; 20.times { |i| t[i] = i }; t[K.new]; p calls
t = {}; 20.times { |i| t["s#{i}"] = i }; t[K.new]; p calls
t = {}; 20.times { |i| t[i * 1.5] = i }; t[K.new]; p calls
h = {}; h[K.new] = 1; p calls
lit = {0=>0,1=>1,2=>2,3=>3,4=>4,5=>5,6=>6,7=>7,8=>8,9=>9,10=>10,11=>11,12=>12,13=>13,14=>14,15=>15,16=>16,17=>17}
lit[K.new]; p calls
# the 17th key builds the index: every key is hashed, then the new one is probed for
g = {}; 16.times { |i| g[i] = i }; g[K.new] = 1; p calls
# the stored key answers eql? in assoc, the argument in []
s = Class.new { def initialize(n) @n = n end; def eql?(o) $calls << [:eql, @n]; false end; def hash; 1 end }
h = {s.new(1) => 1, s.new(2) => 2}; h.assoc(:x); p calls
h[s.new(3)]; p calls
# shift asks the first key for its code once there is an index
x = {}; 17.times { |i| x[i] = i }; k = K.new; x.delete(0); x[k] = 1
16.times { x.shift }; calls; x.shift; p calls
# rehash stores each entry again
r = {K.new => 1, K.new => 2}; calls; r.rehash; p calls.map(&:class)
# a lookup whose eql? deletes entries raises
ev = Class.new { def initialize(h) @h = h end; def eql?(o) @h.delete("k1"); false end; def hash; 0 end }
m = {"k0" => 0, "k1" => 1}
p(begin; m[ev.new(m)]; rescue => e; [e.class, e.message]; end)
# a Set compares with every element up to four of them, the stored one answering
st = Set.new([s.new(:a), s.new(:b)]); calls; st.include?(s.new(:c)); p calls
