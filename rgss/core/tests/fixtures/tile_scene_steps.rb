viewport = Viewport.new(0, 0, 64, 64)
$tile_renderer = TilemapRenderer.new(viewport)
$tile_renderer.tilesets["atlas"] = Bitmap.new(256, 64)
$tile_renderer.autotiles.instance_variable_get(:@frame_durations)["water"] = 0.25
$tile_renderer.autotiles["water"] = Bitmap.new(64, 32)

def tile_state
  tile = $tile_renderer.instance_variable_get(:@tiles)[0][0][0]
  puts [tile.tile_id, tile.x, tile.y, tile.src_rect.x, tile.src_rect.y,
        tile.tone.red.to_i, tile.color.alpha.to_i, tile.need_refresh].join(",")
end

$tile_renderer.update
tile_state
3.times { $tile_renderer.update }
tile_state
$game_map.data[0, 0, 0] = 385
$tile_renderer.update
tile_state
$game_map.display_x = 4
$tile_renderer.update
tile_state
$tile_renderer.tone = Tone.new(10, 20, 30, 0)
$tile_renderer.color = Color.new(20, 30, 40, 50)
$tile_renderer.update
tile_state
$tile_renderer.viewport.ox = 2
$tile_renderer.update
tile_state
$game_map.data[0, 0, 0] = 48
$tile_renderer.update
tile_state
$tile_clock = 0.26
$tile_renderer.update
tile_state
$PokemonSystem.autotile_animations = 1
$tile_clock = 0.75
$tile_renderer.update
tile_state
$tile_renderer.refresh
$tile_renderer.update
tile_state
GC.start
$tile_renderer.update
tile_state
