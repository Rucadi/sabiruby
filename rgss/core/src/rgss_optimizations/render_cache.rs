use std::sync::Arc;

/// Backend-neutral renderable sprite data. Backends may upload `image` or use
/// it in a software compositor; no GPU handles are stored here.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderSprite {
    pub asset_key: Arc<str>,
    pub image: Arc<[u8]>,
    pub raw_rgba: Option<(u32, u32)>,
    pub bitmap_width: u32,
    pub bitmap_height: u32,
    pub src_x: i64,
    pub src_y: i64,
    pub src_width: i64,
    pub src_height: i64,
    pub x: i64,
    pub y: i64,
    pub ox: i64,
    pub oy: i64,
    pub zoom_x: f64,
    pub zoom_y: f64,
    pub opacity: u8,
    pub angle: f64,
    pub mirror: bool,
    pub color: [u8; 4],
    pub tone: [i16; 4],
    pub blend_type: i64,
    pub visible: bool,
    pub z: i64,
    pub tile: bool,
    pub clip_rect: Option<[i64; 4]>,
}

/// Backend-neutral result of comparing one frame with the retained frame.
/// A backend decides how to turn this into buffers, textures, or software
/// draw commands.
#[derive(Clone, Debug, Default)]
pub struct RenderCachePlan {
    /// Shared so producing a plan does not copy one boolean per sprite every
    /// frame. A new slice is allocated only when the eligibility state
    /// changes.
    pub static_eligible: Arc<[bool]>,
    pub static_offset: [i64; 2],
    pub static_rebuild: bool,
}

#[derive(Default)]
pub struct RenderCache {
    frames: Vec<RenderSprite>,
    eligible: Arc<[bool]>,
    offset: [i64; 2],
    logical_size: Option<(u32, u32)>,
}

impl RenderCache {
    pub fn prepare(
        &mut self,
        sprites: &[RenderSprite],
        logical_width: u32,
        logical_height: u32,
    ) -> RenderCachePlan {
        let logical_size_changed = self.logical_size != Some((logical_width, logical_height));
        if logical_size_changed {
            self.frames.clear();
            self.eligible = Arc::from([]);
            self.offset = [0, 0];
            self.logical_size = Some((logical_width, logical_height));
        }
        let mismatches = if self.frames.len() == sprites.len() {
            self.frames
                .iter()
                .zip(sprites)
                .filter(|(cached, current)| !same_sprite(cached, current))
                .count()
        } else {
            sprites.len()
        };
        let mut candidates: Vec<((i64, i64), usize)> = Vec::new();
        for (cached, current) in self.frames.iter().zip(sprites) {
            if !same_sprite(cached, current) && same_sprite_except_position(cached, current) {
                let delta = (current.x - cached.x, current.y - cached.y);
                if let Some((_, count)) = candidates.iter_mut().find(|(known, _)| *known == delta) {
                    *count += 1;
                } else {
                    candidates.push((delta, 1));
                }
            }
        }
        let (candidate, translated_count) = candidates
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map_or((None, 0), |(delta, count)| (Some(delta), count));
        let translation =
            candidate.filter(|_| translated_count >= 8 && translated_count * 2 >= mismatches);
        let static_rebuild = logical_size_changed
            || self.frames.len() != sprites.len()
            || (mismatches > sprites.len().saturating_div(2) && translation.is_none());
        if static_rebuild {
            self.eligible = vec![true; sprites.len()].into();
            self.offset = [0, 0];
            self.frames = sprites.to_vec();
        } else if let Some(delta) = translation {
            self.offset = [delta.0, delta.1];
            self.eligible = self
                .frames
                .iter()
                .zip(sprites)
                .map(|(cached, current)| {
                    same_sprite_except_position(cached, current)
                        && current.x - cached.x == delta.0
                        && current.y - cached.y == delta.1
                })
                .collect::<Vec<_>>()
                .into();
        } else if mismatches == 0 {
            self.offset = [0, 0];
            self.eligible = vec![true; sprites.len()].into();
        } else {
            let offset = self.offset;
            self.eligible = self
                .frames
                .iter()
                .zip(sprites)
                .map(|(cached, current)| {
                    if offset != [0, 0] {
                        same_sprite_except_position(cached, current)
                            && current.x - cached.x == offset[0]
                            && current.y - cached.y == offset[1]
                    } else {
                        same_sprite(cached, current)
                    }
                })
                .collect::<Vec<_>>()
                .into();
        }
        RenderCachePlan {
            static_eligible: self.eligible.clone(),
            static_offset: self.offset,
            static_rebuild,
        }
    }
}

fn same_sprite_except_position(a: &RenderSprite, b: &RenderSprite) -> bool {
    a.asset_key == b.asset_key
        && Arc::ptr_eq(&a.image, &b.image)
        && a.raw_rgba == b.raw_rgba
        && a.bitmap_width == b.bitmap_width
        && a.bitmap_height == b.bitmap_height
        && a.src_x == b.src_x
        && a.src_y == b.src_y
        && a.src_width == b.src_width
        && a.src_height == b.src_height
        && a.ox == b.ox
        && a.oy == b.oy
        && a.zoom_x == b.zoom_x
        && a.zoom_y == b.zoom_y
        && a.opacity == b.opacity
        && a.angle == b.angle
        && a.mirror == b.mirror
        && a.color == b.color
        && a.tone == b.tone
        && a.blend_type == b.blend_type
        && a.visible == b.visible
        && a.z == b.z
        && a.tile == b.tile
        && a.clip_rect == b.clip_rect
}

fn same_sprite(a: &RenderSprite, b: &RenderSprite) -> bool {
    same_sprite_except_position(a, b) && a.x == b.x && a.y == b.y
}
