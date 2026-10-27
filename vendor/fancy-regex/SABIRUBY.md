# SabiRuby patch

This is `fancy-regex` 0.19.2 (MIT), vendored because SabiRuby uses it as the
backtracking fallback for regexp constructs a finite automaton cannot express
(look-around, backreferences and atomic groups; see `docs/design/gems.md`).

One change, in `src/vm.rs` (`Insn::Backref`): a case-insensitive Unicode
backreference can make a delegated engine report an inverted capture
(`lo > hi`), and the original code panicked slicing the haystack with it. The
patch treats such a state as a failed backreference instead. The mruby test
suite's `gem_regexp_utf8` case (`"sſ" =~ /(.)\1/i`) exercises it.

Remove the patch when a published fancy-regex handles that state itself.
