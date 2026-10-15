# Three differences that showed in the reference column of the edge comparison (mruby's own
# assert.rb over 891d1d9d's test/t/syntax.rb), past where mruby/edge's driver stops: the send an
# operator instruction falls back to is checked for visibility unless the receiver is the
# caller's own self (mruby 891d1d9d 90e568e8e); a hash pattern whose #deconstruct_keys answers
# no Hash raises TypeError (`Kernel#__pat_values`, `obj_pat_values`); a `defined?` answer is a
# frozen string (033109642). Expectation: the output of mruby 891d1d9d (Docker
# kishima/mruby:891d1d9d).
class PrivOp
  def own_aref; self[1]; end
  def own_plus; self + 1; end
  def aref(o); o[1]; end
  def plus(o); o + 1; end
  def lt(o); o < 1; end
  private
  def [](i); i; end
  def +(o); :plus; end
  def <(o); :lt; end
end
def try
  yield
rescue => e
  "#{e.class}: #{e.message}"
end
o = PrivOp.new
p [o.own_aref, o.own_plus]
p [try { o.aref(PrivOp.new) }, try { o.plus(PrivOp.new) }, try { o.lt(PrivOp.new) }, try { o[0] }, try { o + 1 }]
bad = Class.new { def deconstruct_keys(keys); :nothash; end }.new
good = Class.new { def deconstruct_keys(keys); {a: 1}; end }.new
p [try { bad in {a: 1} }, (good in {a: 1})]
# only the answers the VM makes: rc2's compiler, which this case is compiled with, writes the
# literal ones ("local-variable", "assignment") as plain strings, 891d1d9d's freezes them
p [defined?(String).frozen?, defined?(puts).frozen?, defined?(@iv).nil?]
