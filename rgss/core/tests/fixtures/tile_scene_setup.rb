module GameData
  module TerrainTag
    def self.try_get(_); nil; end
  end
end
module MapFactoryHelper
  def self.hasConnections?(_); false; end
end
def nil_or_empty?(value); value.nil? || value.empty?; end
class TestMapData
  attr_accessor :width, :height, :data, :tileset_id
  def initialize
    @width = @height = 3
    @tileset_id = 1
    @data = Table.new(3, 3, 3)
    3.times { |x| 3.times { |y| @data[x, y, 0] = 384 } }
  end
end
class Game_Map
  TILE_WIDTH = TILE_HEIGHT = 32
  X_SUBPIXELS = Y_SUBPIXELS = 4
  attr_accessor :map_id, :display_x, :display_y
  attr_reader :terrain_tags, :priorities, :autotile_names, :tileset_name
  def initialize
    @map = TestMapData.new
    @map_id = 1
    @display_x = @display_y = 0
    @terrain_tags = Table.new(1024)
    @priorities = Table.new(1024)
    @autotile_names = ["water"]
    @tileset_name = "atlas"
  end
  def data; @map.data; end
  def width; @map.width; end
  def height; @map.height; end
  def tileset_id; @map.tileset_id; end
end
class PokemonMapFactory
  attr_accessor :maps
  def initialize; @maps = [$game_map]; end
end
class TestSettings
  attr_accessor :bridge, :autotile_animations
  def initialize; @bridge = @autotile_animations = 0; end
end
$tile_clock = 0.0
module System
  def self.uptime; $tile_clock; end
end
$PokemonGlobal = TestSettings.new
$PokemonSystem = TestSettings.new
$game_map = Game_Map.new
$map_factory = PokemonMapFactory.new
Graphics.resize_screen(64, 64)
