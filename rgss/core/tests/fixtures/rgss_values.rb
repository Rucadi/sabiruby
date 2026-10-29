def describe_rgss_values
  tiles, passages, tone, color, rect = $rgss_values
  puts [tiles.xsize, tiles.ysize, tiles.zsize, tiles[0, 0, 0],
        tiles[1, 0, 0], tiles[0, 1, 1], tiles[1, 1, 1]].join(",")
  # The first tile is walkable, the second blocks all four directions.
  puts [passages[tiles[0, 0, 0]] & 15, passages[tiles[1, 0, 0]] & 15].join(",")
  puts [tone.red.to_i, tone.green.to_i, tone.blue.to_i, tone.gray.to_i].join(",")
  puts [color.red.to_i, color.green.to_i, color.blue.to_i, color.alpha.to_i].join(",")
  puts [rect.x, rect.y, rect.width, rect.height].join(",")
end

tiles = Table.new(2, 2, 2)
tiles[0, 0, 0] = 384
tiles[1, 0, 0] = 385
tiles[0, 1, 1] = 65535
tiles[1, 1, 1] = -32769
passages = Table.new(386)
passages[385] = 15
$rgss_values = [tiles, passages, Tone.new(0, -20, 30, 40),
                Color.new(20, 40, 60, 128), Rect.new(-2, 3, 32, 64)]
describe_rgss_values
