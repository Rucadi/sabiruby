tone = Tone.new(0, -20, 30, 40)
same_tone = tone.clone
puts [tone == same_tone, tone != same_tone, tone === same_tone,
      tone == nil, tone == Color.new(0, 0, 0, 0)].join(",")
same_tone.gray = 41
puts tone == same_tone
color = Color.new(20, 40, 60, 128)
same_color = color.clone
puts [color == same_color, color != same_color, color === same_color].join(",")
same_color.alpha = 0
puts color == same_color
rect = Rect.new(1, 2, 32, 64)
same_rect = rect.clone
puts [rect == same_rect, rect != same_rect, rect === same_rect].join(",")
same_rect.x = 2
puts rect == same_rect

# TilemapRenderer keeps clones of these values to detect changed effects.
old_tone = tone.clone
old_color = color.clone
updates = 0
100.times do
  updates += 600 if old_tone != tone
  updates += 600 if old_color != color
  old_tone = tone.clone
  old_color = color.clone
end
puts updates
