# Assertion driver for the unmodified mruby test/t suite. Keep compatibility
# code here, rather than adding methods to the core classes under test.
module Mrbtest
  FLOAT_TOLERANCE = 1e-10
end
class MRubyTestSkip < Exception; end
class MRubyTestStress < MRubyTestSkip; end
$undefined = Object.new
$ok_test = 0
$ko_test = 0
$kill_test = 0
$skip_test = 0
$stress_test = 0
$warning_test = 0
$spec_checks = 0
$spec_failures = 0
$spec_assert_id = 0

def assert(*args, &block)
  description = args[0] || 'assert'
  $spec_assert_id += 1
  id = $spec_assert_id
  _spec_event('start', id, description)
  before = $spec_failures
  checks = $spec_checks
  t_print(description, ' : ')
  begin
    block.call
    if $spec_failures > before
      _spec_event('fail', id, nil)
      $ko_test += 1
      t_print("FAIL\n")
    elsif $spec_checks == checks
      _spec_event('warn', id, 'no assertions')
      $warning_test += 1
      t_print("WARN (no assertions)\n")
    else
      _spec_event('pass', id, nil)
      $ok_test += 1
      t_print("OK\n")
    end
  rescue MRubyTestStress
    _spec_event('stress', id, 'stress test disabled')
    $stress_test += 1
    t_print("STRESS (not run)\n")
  rescue MRubyTestSkip => e
    _spec_event('skip', id, e.message)
    $skip_test += 1
    t_print("SKIP: ", e.message, "\n")
  rescue Exception => e
    _spec_event('error', id, e.message)
    $kill_test += 1
    $spec_failures += 1
    t_print("ERROR: ", e.message, "\n")
  end
end

def assert_true(value, message = nil, diff = nil)
  $spec_checks += 1
  if value == true
    true
  else
    $spec_failures += 1
    _spec_event('check_failure', 0, message, diff)
    t_print("\n  assertion failed: ", message, ' ', diff, "\n")
    false
  end
end

def assert_false(value, message = nil, diff = nil)
  assert_true(value == false, message, diff)
end

def assertion_diff(expected, actual)
  "expected #{expected.inspect}, actual #{actual.inspect}"
end

def assert_equal(*args, &block)
  expected = args[0]
  actual = args[1]
  message = args[2]
  if block
    message = actual
    actual = block.call
  end
  result = expected == actual
  assert_true(result, message, result ? nil : assertion_diff(expected, actual))
end

def assert_not_equal(*args, &block)
  expected = args[0]
  actual = args[1]
  message = args[2]
  if block
    message = actual
    actual = block.call
  end
  assert_false(expected == actual, message)
end

def assert_same(expected, actual, message = nil)
  assert_true(expected.equal?(actual), message)
end

def assert_not_same(expected, actual, message = nil)
  assert_false(expected.equal?(actual), message)
end

def assert_nil(value, message = nil)
  assert_true(value.nil?, message)
end

def assert_not_nil(value, message = nil)
  assert_false(value.nil?, message)
end

def assert_include(collection, value, message = nil)
  assert_true(collection.include?(value), message)
end

def assert_not_include(collection, value, message = nil)
  assert_false(collection.include?(value), message)
end

def assert_predicate(value, method, message = nil)
  assert_true(value.__send__(method), message)
end

def assert_not_predicate(value, method, message = nil)
  assert_false(value.__send__(method), message)
end

def assert_operator(left, method, right = $undefined, message = nil)
  if right.equal?($undefined)
    assert_predicate(left, method, message)
  else
    assert_true(left.__send__(method, right), message)
  end
end

def assert_not_operator(left, method, right = $undefined, message = nil)
  if right.equal?($undefined)
    assert_not_predicate(left, method, message)
  else
    assert_false(left.__send__(method, right), message)
  end
end

def assert_match(pattern, value, message = nil)
  assert_true(_str_match?(pattern, value), message)
end

def assert_not_match(pattern, value, message = nil)
  assert_false(_str_match?(pattern, value), message)
end

def assert_kind_of(klass, value, message = nil)
  assert_true(value.kind_of?(klass), message)
end

def assert_float(expected, actual, message = nil)
  e = expected.to_f
  a = actual.to_f
  if e.nan?
    assert_true(a.nan?, message)
  elsif e.infinite?
    assert_equal(e, a, message)
  elsif a.finite?
    assert_true((e - a).abs <= Mrbtest::FLOAT_TOLERANCE, message)
  else
    flunk(message)
  end
end

def assert_raise(*exceptions, &block)
  message = exceptions.last.is_a?(String) ? exceptions.pop : nil
  exceptions = [StandardError] if exceptions.empty?
  begin
    block.call
  rescue Exception => e
    matched = false
    exceptions.each { |klass| matched = true if e.kind_of?(klass) }
    assert_true(matched, message)
    return matched ? e : nil
  end
  flunk(message, 'expected exception, but nothing was raised')
  nil
end

def assert_nothing_raised(*args, &block)
  message = args[0]
  caught = nil
  begin
    block.call
  rescue Exception => e
    caught = e
  end
  caught ? flunk(message, caught.message) : pass
end

def assert_raise_with_message(*args, &block)
  klass = args[0]
  message = args[1]
  description = args[2]
  e = assert_raise(klass, &block)
  assert_equal(message, e.message, description) if e
end

def assert_raise_with_message_pattern(*args, &block)
  klass = args[0]
  pattern = args[1]
  description = args[2]
  e = assert_raise(klass, &block)
  assert_match(pattern, e.message, description) if e
end

def pass
  assert_true(true)
end

def flunk(message = 'Epic Fail!', diff = '')
  assert_true(false, message, diff)
end

def skip(cause = '')
  raise MRubyTestSkip, cause
end

def stress
  raise MRubyTestStress, 'stress test' unless $mrbtest_stress
end

def report
  t_print("OK: #{$ok_test}, FAIL: #{$ko_test}, ERROR: #{$kill_test}, WARN: #{$warning_test}, SKIP: #{$skip_test}, STRESS: #{$stress_test}\n")
  $spec_failures == 0
end
