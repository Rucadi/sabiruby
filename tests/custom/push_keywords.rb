# `Array#push` given keyword arguments. mruby 4.1.0-rc2 (and master 891d1d9d) answers `[1]` for
# `[].push(1, k: 2)`: the keyword Hash is dropped, although mruby hands it to a C function whose
# format takes no keywords as one more positional argument everywhere else (`mrb_get_args`
# without ":"). Arguments disappearing without an error is clearly a defect, so SabiRuby keeps
# the Hash as the last element, as CRuby does (the author's decision of 2026-10-18).
# expected-from: CRuby 3.2. The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = begin
    yield.inspect
  rescue Exception => e
    "#{e.class}: #{e.message}"
  end
  puts "#{label} => #{r}"
end
t("[].push(1, k: 2)") { [].push(1, k: 2) }
t("[1].push(k: 1)") { [1].push(k: 1) }
t("[].push({k: 2})") { [].push({k: 2}) }
t("[].push(1, **{})") { [].push(1, **{}) }
