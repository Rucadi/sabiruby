
# The loop of one chunk of the method sweep, appended by sweep.py after prelude.rb and the
# chunk's SW_CASES_SRC. The case to start from (the driver starts again after the case that
# hung or crashed) is ARGV[0] on the reference and SW_START on SabiRuby, where the chunk runs
# under `sabiruby edge-spec start.mrb chunk.mrb`: the `sabiruby` command keeps the output until
# the program ends, and edge-spec's `t_print` writes it at once, so a case that hangs or kills
# the process does not take the lines before it along. One line per case, "\x01" first so that nothing else on the output
# can be read as a result:
#   index TAB ok TAB value.inspect TAB "" TAB receiver.inspect TAB what the block was given
#   index TAB raise TAB exception class TAB message TAB receiver.inspect TAB what the block was given
SW_CASES = SW_CASES_SRC.split("\n").map do |l|
  rk, m, ai, kw, blk = l.split("\t", -1)
  [rk, m.to_sym, ai.split(",").map(&:to_i), kw == "1", blk == "1"]
end
if respond_to?(:t_print, true)
  def sw_out(s) = t_print(s + "\n")
else
  def sw_out(s) = print(s + "\n")
end
sw_out "\x01ready"
i = defined?(SW_START) ? SW_START : ARGV[0].to_i
while i < SW_CASES.size
  rk, m, ai, kw, blk = SW_CASES[i]
  srand(1)
  r = SW_RECV[rk].call
  a = ai.map { |j| SW_VALS[j].call }
  log = []
  b = blk ? proc { |*x| log << x; raise SwStop if log.size > SW_YIELD_MAX; x[0] } : nil
  begin
    v = kw ? r.__send__(m, *a, k: 1, &b) : r.__send__(m, *a, &b)
    res = "ok\t#{sw_insp(v)}\t"
  rescue SwStop
    res = "stopped\t#{SW_YIELD_MAX}\t"
    log = log.first(3)
  rescue Exception => e
    msg = begin; sw_esc(e.message.to_s); rescue Exception; "<message raised>"; end
    res = "raise\t#{e.class}\t#{msg}"
  end
  sw_out "\x01#{i}\t#{res}\t#{sw_insp(r)}\t#{sw_insp(log)}"
  i += 1
end
sw_out "\x01end"
$spec_completed = true
