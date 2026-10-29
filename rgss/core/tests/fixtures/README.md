`input_selection.rb` exercises the numeric buttons serialized by RPG Maker XP
in event command 111: left = 4, right = 6, confirm = 13. Anil's Map001 EV004
uses these exact values. The fixture has no dependency on game scripts or assets.

The core test supplies right for one update, then releases right and presses
confirm. `input_selection.out` is the expected trace, also checked against
`../rsxp/crates/fanplay-rgss/src/ruby/rgss/input.rb` with the equivalent SDL
scancode snapshots (79 for right, 29 for Z/confirm).

`sprite_order.rb` models the naming screen's overlapping shadow, walking
character and UI without game assets. The expected back-to-front bitmap widths
are `512,44,48,288,16`, even after moving the shadow above the character.
This matches the reference ordering in
`../rsxp/crates/fanplay-rgss/src/engine/render_software.rs`: `compose_scene`
and `draw_viewport` sort by `(z, order)`, explicitly ignoring y. The regression
checks this for both root sprites and sprites in a viewport. Sorting by y would
instead draw the shadow over the character's feet and move later overlays behind
earlier sprites.

`marshal_choices.rb` builds an event-command graph with nested choices, shared
references, a cycle, wide/negative integers, floats and a hash default. Both
`marshal_choices.out` and `marshal_choices.marshal` were generated with MRI Ruby
4.0.9. The core regression checks the Ruby behavior and byte-for-byte Marshal
4.8 output. Regenerate the references with:

```sh
ruby -e 'load ARGV[0]; File.binwrite(ARGV[1], Marshal.dump($marshal_graph))' \
  rgss/core/tests/fixtures/marshal_choices.rb \
  rgss/core/tests/fixtures/marshal_choices.marshal \
  > rgss/core/tests/fixtures/marshal_choices.out
```

`rgss_values.rb` and `rgss_values.marshal` check 3D tile cells, 1D passage flags,
signed 16-bit writes, and serialized Tone/Color/Rect components. The `.out` and
binary references were generated with MRI Ruby 4.0.9 using rsxp's portable RGSS
classes. Regenerate from this repository with:

```sh
ruby -e 'module FANPLAY; end; \
  load "../rsxp/crates/fanplay-rgss/src/ruby/rgss/values.rb"; \
  load "../rsxp/crates/fanplay-rgss/src/ruby/rgss/table.rb"; \
  load "rgss/core/tests/fixtures/rgss_values.rb"; \
  File.binwrite("rgss/core/tests/fixtures/rgss_values.marshal", Marshal.dump($rgss_values))' \
  > rgss/core/tests/fixtures/rgss_values.out
```
