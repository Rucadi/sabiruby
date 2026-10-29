# RPG Maker XP serializes numeric button IDs in event command 111.
# This fixture also runs against rsxp/mkxp: use LEFT/RIGHT then confirm.
puts [Input::DOWN, Input::LEFT, Input::RIGHT, Input::UP,
      Input::A, Input::B, Input::C, Input::X, Input::Y,
      Input::Z, Input::L, Input::R].join(',')
selection = 1
loop do
  Graphics.update
  Input.update
  selection = (selection + 2) % 3 if Input.trigger?(4)
  selection = (selection + 1) % 3 if Input.trigger?(6)
  if Input.press?(13)
    puts selection
    break
  end
end
