# Synthetic naming-screen geometry: the subject overlaps its earlier shadow.
# Distinct bitmap widths identify each layer in the render snapshot.
def build_sprite_order_scene(viewport)
  $order_sprites = []
  # Creation order deliberately differs from both y order and z order.
  [[16, 16, 0, 10],       # foreground
   [44, 12, 64, 0],       # shadow, created before the character
   [48, 64, 12, 0],       # walking character, feet at y = 76
   [288, 96, 0, 0],       # later UI overlay
   [512, 384, 0, -10]     # background
  ].each do |width, height, y, z|
    sprite = Sprite.new(viewport)
    sprite.bitmap = Bitmap.new(width, height)
    sprite.y = y
    sprite.z = z
    $order_sprites << sprite
  end
end
