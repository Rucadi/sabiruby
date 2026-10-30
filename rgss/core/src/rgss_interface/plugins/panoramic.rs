use crate::{RgssError, RgssRuntime, RgssState};

pub(crate) fn set_render_size(runtime: &mut RgssRuntime, width: u32, height: u32) {
    let width = width.max(1);
    let height = height.max(1);
    let panoramic = width > runtime.project.screen_width || height > runtime.project.screen_height;
    let changed = runtime
        .vm
        .host_state::<RgssState>()
        .is_none_or(|state| state.width != width || state.height != height);
    if let Some(state) = runtime.vm.host_state_mut::<RgssState>() {
        state.width = width;
        state.height = height;
        state.panoramic_render = panoramic;
    }
    if !changed {
        return;
    }

    let resize_source: &[u8] = if panoramic {
        br#"
$__sabiruby_panoramic_active = true
if defined?(Game_Player) && Game_Player.method_defined?(:center) &&
   Game_Player.method_defined?(:update_screen_position) &&
   !Game_Player.method_defined?(:__sabiruby_panoramic_center)
  Game_Player.class_eval do
    alias_method :__sabiruby_panoramic_center, :center
    alias_method :__sabiruby_panoramic_update_screen_position, :update_screen_position
    def center(x, y)
      if $__sabiruby_panoramic_active && defined?(Game_Map) &&
         Game_Map.const_defined?(:REAL_RES_X) && Game_Map.const_defined?(:REAL_RES_Y) &&
         Game_Map.const_defined?(:TILE_WIDTH) && Game_Map.const_defined?(:TILE_HEIGHT)
        cx = ((Graphics.width / 2) - (Game_Map::TILE_WIDTH / 2)) * Game_Map::X_SUBPIXELS
        cy = ((Graphics.height / 2) - (Game_Map::TILE_HEIGHT / 2)) * Game_Map::Y_SUBPIXELS
        self.map.display_x = (x * Game_Map::REAL_RES_X) - cx
        self.map.display_y = (y * Game_Map::REAL_RES_Y) - cy
      else
        __sabiruby_panoramic_center(x, y)
      end
    end
    def update_screen_position(last_real_x, last_real_y)
      unless $__sabiruby_panoramic_active && defined?(Game_Map) &&
             Game_Map.const_defined?(:REAL_RES_X) && Game_Map.const_defined?(:REAL_RES_Y)
        return __sabiruby_panoramic_update_screen_position(last_real_x, last_real_y)
      end
      return if self.map.scrolling? || !(@moved_last_frame || @moved_this_frame)
      center_x = ((Graphics.width / 2) - (Game_Map::TILE_WIDTH / 2)) * Game_Map::X_SUBPIXELS
      center_y = ((Graphics.height / 2) - (Game_Map::TILE_HEIGHT / 2)) * Game_Map::Y_SUBPIXELS
      if (@real_x < last_real_x && @real_x < $game_map.display_x + center_x) ||
         (@real_x > last_real_x && @real_x > $game_map.display_x + center_x)
        self.map.display_x += @real_x - last_real_x
      end
      if (@real_y < last_real_y && @real_y < $game_map.display_y + center_y) ||
         (@real_y > last_real_y && @real_y > $game_map.display_y + center_y)
        self.map.display_y += @real_y - last_real_y
      end
    end
  end
end
if defined?($scene) && $scene && $scene.respond_to?(:disposeSpritesets) && $scene.respond_to?(:createSpritesets)
  $scene.disposeSpritesets
  renderer = $scene.instance_variable_get(:@map_renderer) if $scene.respond_to?(:instance_variable_get)
  renderer.dispose if renderer && renderer.respond_to?(:dispose) &&
                     (!renderer.respond_to?(:disposed?) || !renderer.disposed?)
  $scene.instance_variable_set(:@map_renderer, nil) if $scene.respond_to?(:instance_variable_set)
  $scene.createSpritesets
end
if defined?($game_player) && defined?(Game_Map) &&
   Game_Map.const_defined?(:REAL_RES_X) && Game_Map.const_defined?(:REAL_RES_Y) &&
   Game_Map.const_defined?(:TILE_WIDTH) && Game_Map.const_defined?(:TILE_HEIGHT)
  cx = ((Graphics.width / 2) - (Game_Map::TILE_WIDTH / 2)) * Game_Map::X_SUBPIXELS
  cy = ((Graphics.height / 2) - (Game_Map::TILE_HEIGHT / 2)) * Game_Map::Y_SUBPIXELS
  $game_player.map.display_x = ($game_player.x * Game_Map::REAL_RES_X) - cx
  $game_player.map.display_y = ($game_player.y * Game_Map::REAL_RES_Y) - cy
end
if defined?(Spriteset_Map) && !Spriteset_Map.method_defined?(:__sabiruby_panoramic_update)
  Spriteset_Map.class_eval do
    alias_method :__sabiruby_panoramic_update, :update
    def update
      if $__sabiruby_panoramic_active && defined?(@@viewport0) && @@viewport0 && !@@viewport0.disposed?
        @@viewport0.rect.set(0, 0, Graphics.width, Graphics.height)
      end
      if $__sabiruby_panoramic_active && defined?(@@viewport3) && @@viewport3 && !@@viewport3.disposed?
        @@viewport3.rect.set(0, 0, Graphics.width, Graphics.height)
      end
      __sabiruby_panoramic_update
    end
  end
end
"#
    } else {
        br#"
$__sabiruby_panoramic_active = false
if defined?($game_player) && defined?(Game_Map) &&
   Game_Map.const_defined?(:REAL_RES_X) && Game_Map.const_defined?(:REAL_RES_Y) &&
   $game_player.respond_to?(:center)
  $game_player.center($game_player.x, $game_player.y)
end
if defined?($scene) && $scene && $scene.respond_to?(:disposeSpritesets) && $scene.respond_to?(:createSpritesets)
  $scene.disposeSpritesets
  renderer = $scene.instance_variable_get(:@map_renderer) if $scene.respond_to?(:instance_variable_get)
  renderer.dispose if renderer && renderer.respond_to?(:dispose) &&
                     (!renderer.respond_to?(:disposed?) || !renderer.disposed?)
  $scene.instance_variable_set(:@map_renderer, nil) if $scene.respond_to?(:instance_variable_set)
  $scene.createSpritesets
end
"#
    };
    let _: Result<_, RgssError> = runtime.run_source("__sabiruby_panoramic_resize", resize_source);
}
