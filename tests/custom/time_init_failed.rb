# A Time keeps its value when `initialize` is called on it again and fails (rc2-sweep cause
# `time-init-failed`). mruby 4.1.0-rc2's `time_init` (mrbgems/mruby-time/src/time.c:1216) reads
# its arguments, then frees the Time's data and sets it to NULL (`mrb_data_init(self, NULL,
# &time_type)`, :1228) before `time_mktime` checks the values; when that raises ("argument out of
# range"), the receiver is left with no data and every later call on it raises ArgumentError
# "uninitialized Time". A failed call loses the receiver's value (the compatibility policy:
# data is lost, plan rc2-sweep-fixes Y6b, 2026-10-18). An argument `mrb_get_args` refuses (nil)
# raises before the data is freed, so rc2 keeps the value there.
# expected-from: SabiRuby (CRuby 3.2 keeps the value too, and says "mon out of range").
# The reference's answers are in .rc.out. README, "Known deviations".
def t(label)
  r = yield
  puts "#{label}: #{r.inspect}"
rescue Exception => e
  puts "#{label}: #{e.class}: #{e.message}"
end

tm = Time.at(0).utc
t("initialize(0, 0)") { tm.__send__(:initialize, 0, 0) }
t("inspect after") { tm.inspect }
t("year after") { tm.year }
tm2 = Time.at(0).utc
t("initialize(nil)") { tm2.__send__(:initialize, nil) }
t("inspect after nil") { tm2.inspect }
