# A multiple assignment from a value that is not a literal (OP_AREF per target, and OP_APOST
# for a splat) and a block whose two parameters spread one Array argument (OP_ENTER). These
# ask whether the value is an Array in the reference's sense (`mrb_array_p`, so not a Struct)
# since rc2-leftovers5. Same loop and count as m_loop_only.rb.
pair = [1, 2]
trio = [1, 2, 3]
blk = proc { |x, y| x }
n = 0
i = 0
while i < 200_000
  a, b = pair
  c, *d = trio
  n += blk.call(pair) + a + b + c
  i += 1
end
