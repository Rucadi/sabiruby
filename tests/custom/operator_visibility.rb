# Three answers of mruby 4.1.0-rc2 that 891d1d9d changed after the stable branch point: the send
# an operator instruction falls back to is not checked for visibility (891d1d9d checks it unless
# the receiver is the caller's own self, 90e568e8e); a hash pattern whose #deconstruct_keys
# answers no Hash sends `__pat_values` to that answer (891d1d9d raises TypeError, b2d2cd15e); a
# `defined?` answer is not frozen (891d1d9d freezes it, 033109642). Expectation: mruby 4.1.0-rc2
# (.rc.out), which SabiRuby keeps (docs/worklog/2026-10-16-follow-master.md).
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
