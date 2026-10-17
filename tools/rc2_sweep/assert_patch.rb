
# Loaded after mruby's test/assert.rb by programs.py (docs/plans/rc2-sweep-plan.md X2), on both
# VMs alike: what the reference's test driver (mrbgems/mruby-test/driver.c) defines in C, written
# in Ruby, and one line per `assert` so that the two runs are compared assertion by assertion.

# driver.c: FLOAT_TOLERANCE of a build with double Floats
module Mrbtest
  FLOAT_TOLERANCE = 1e-10
end

# driver.c t_print writes the progress marks; here the last one is kept as the assertion's result
def t_print(*a)
  $sw_last = a.last
  nil
end

# driver.c str_match_p and str_match_no_brace_p: the glob of assert_match, with {a,b} braces
def _sw_glob(pat, str)
  pi = si = 0
  pt = st = nil
  while true
    if pi == pat.size
      return true if si == str.size
    elsif pat[pi] == "*"
      pt = pi; st = si; pi += 1
      next
    elsif si < str.size
      c = pat[pi]
      if c == "?"
        pi += 1; si += 1
        next
      elsif c == "["
        j = pi + 1
        neg = pat[j] == "^" || pat[j] == "!"
        j += 1 if neg
        ok = false
        first = true
        while j < pat.size && (first || pat[j] != "]")
          first = false
          a = pat[j]
          if a == "\\" && j + 1 < pat.size
            j += 1; a = pat[j]
          end
          if pat[j + 1] == "-" && j + 2 < pat.size && pat[j + 2] != "]"
            b = pat[j + 2]
            ok = true if a <= str[si] && str[si] <= b
            j += 3
          else
            ok = true if a == str[si]
            j += 1
          end
        end
        if j < pat.size && ok != neg
          pi = j + 1; si += 1
          next
        end
      else
        if c == "\\" && pi + 1 < pat.size
          pi += 1; c = pat[pi]
        end
        if c == str[si]
          pi += 1; si += 1
          next
        end
      end
    end
    return false unless pt && st < str.size
    pi = pt + 1; st += 1; si = st
  end
end

def _str_match?(pat, str, depth = 0)
  return false if depth > 100   # driver.c STR_MATCH_MAX_BRACE_DEPTH
  nest = 0; lb = rb = nil; i = 0
  while i < pat.size
    c = pat[i]
    if c == "{"
      lb = i if nest == 0
      nest += 1
    elsif c == "}" && lb
      nest -= 1
      if nest == 0
        rb = i
        break
      end
    elsif c == "\\"
      i += 1
    end
    i += 1
  end
  return _sw_glob(pat, str) unless lb && rb
  head, tail = pat[0, lb], pat[rb + 1..-1]
  alts, cur, nest, i = [], "", 0, lb + 1
  while i < rb
    c = pat[i]
    if c == "," && nest == 0
      alts << cur; cur = ""
    else
      nest += 1 if c == "{"
      nest -= 1 if c == "}"
      if c == "\\" && i + 1 < rb
        cur += c; i += 1; c = pat[i]
      end
      cur += c
    end
    i += 1
  end
  alts << cur
  alts.any? { |a| _str_match?(head + a + tail, str, depth + 1) }
end

def _sw_esc(s)
  s.to_s.gsub("\n", "\\n").gsub("\t", "\\t")
end

$sw_seq = 0
$sw_depth = 0
alias _sw_orig_assert assert
def assert(str = 'assert', iso = '', &b)
  depth = $sw_depth
  $sw_depth += 1
  n0 = $asserts.size
  begin
    _sw_orig_assert(str, iso, &b)
  ensure
    $sw_depth -= 1
  end
  st = $sw_last
  msg = st != "." && depth == 0 && $asserts.size > n0 ? $asserts.last.to_s : ""
  msg = msg.split("\nbacktrace:\n")[0]   # backtraces are compared elsewhere (backlog)
  $sw_seq += 1
  print "\x01A\t#{$sw_seq}\t#{depth}\t#{_sw_esc(str)}\t#{st}\t#{_sw_esc(msg)}\n"
end
