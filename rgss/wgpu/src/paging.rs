//! Bitmaps taller than the GPU's texture limit.
//!
//! Older Essentials games keep a whole tileset in one bitmap 256 pixels wide
//! and tens of thousands tall, past the 8192 rows a texture may have. Such a
//! bitmap is cut into horizontal pages of at most the limit, each uploaded as a
//! texture of its own, and every sprite sampling it is replaced by one sprite
//! per page its source rectangle crosses. A sprite's position is kept by moving
//! its origin (`oy`) down by the rows the page skips, which keeps zoom, angle
//! and mirroring exact. Pixels are only copied the first time a page is used.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use sabiruby_rgss_core::RenderSprite;

/// One page: its own asset key, pixels and height.
struct Page {
    key: Arc<str>,
    rgba: Arc<[u8]>,
    height: u32,
}

/// Pages already cut, by the asset key of the whole bitmap.
#[derive(Default)]
pub(crate) struct PageCache {
    pages: HashMap<Arc<str>, Arc<Vec<Page>>>,
}

/// The suffix that names a page of an asset (`bitmap:7:3#page2`).
pub(crate) const PAGE_SUFFIX: &str = "#page";

impl PageCache {
    /// `sprites` with every sprite of an over-tall bitmap replaced by its pages,
    /// or the same slice when no sprite needs it. A bitmap that cannot be paged
    /// (wider than the limit, not raw RGBA, truncated) is passed through, and
    /// the renderer reports it as before.
    pub(crate) fn page<'a>(
        &mut self,
        sprites: &'a [RenderSprite],
        max_dimension: u32,
    ) -> Cow<'a, [RenderSprite]> {
        let needs = |sprite: &RenderSprite| {
            sprite.bitmap_height > max_dimension && sprite.bitmap_width <= max_dimension
        };
        if !sprites.iter().any(needs) {
            return Cow::Borrowed(sprites);
        }
        let mut paged = Vec::with_capacity(sprites.len() + 4);
        for sprite in sprites {
            if !needs(sprite) {
                paged.push(sprite.clone());
                continue;
            }
            let Some(pages) = self.pages_of(sprite, max_dimension) else {
                paged.push(sprite.clone());
                continue;
            };
            let height = i64::from(sprite.bitmap_height);
            let top = sprite.src_y.clamp(0, height);
            let bottom = (sprite.src_y + sprite.src_height).clamp(top, height);
            let mut page_top = 0i64;
            for page in pages.iter() {
                let page_bottom = page_top + i64::from(page.height);
                let (start, end) = (top.max(page_top), bottom.min(page_bottom));
                if start < end {
                    let mut part = sprite.clone();
                    part.asset_key = page.key.clone();
                    part.image = page.rgba.clone();
                    part.raw_rgba = Some((sprite.bitmap_width, page.height));
                    part.bitmap_height = page.height;
                    part.src_y = start - page_top;
                    part.src_height = end - start;
                    // the part starts this many source rows below the sprite's top
                    part.oy = sprite.oy - (start - sprite.src_y);
                    paged.push(part);
                }
                page_top = page_bottom;
            }
        }
        Cow::Owned(paged)
    }

    fn pages_of(&mut self, sprite: &RenderSprite, max_dimension: u32) -> Option<Arc<Vec<Page>>> {
        if let Some(pages) = self.pages.get(&sprite.asset_key) {
            return Some(pages.clone());
        }
        let (width, height) = sprite.raw_rgba?;
        let row = width as usize * 4;
        if sprite.image.len() < row * height as usize || max_dimension == 0 {
            return None;
        }
        let mut pages = Vec::new();
        let mut page_top = 0;
        while page_top < height {
            let page_height = (height - page_top).min(max_dimension);
            let start = page_top as usize * row;
            let end = start + page_height as usize * row;
            pages.push(Page {
                key: Arc::from(format!("{}{PAGE_SUFFIX}{}", sprite.asset_key, pages.len())),
                rgba: Arc::from(&sprite.image[start..end]),
                height: page_height,
            });
            page_top += page_height;
        }
        // Keep only the current revision of a raw bitmap, like the textures.
        if let Some(prefix) = bitmap_prefix(&sprite.asset_key) {
            self.pages.retain(|key, _| !key.starts_with(prefix));
        }
        let pages = Arc::new(pages);
        self.pages.insert(sprite.asset_key.clone(), pages.clone());
        Some(pages)
    }
}

/// `bitmap:ID:` of a raw RGSS bitmap's asset key (`bitmap:ID:REVISION`).
pub(crate) fn bitmap_prefix(key: &str) -> Option<&str> {
    let rest = key.strip_prefix("bitmap:")?;
    let (id, _) = rest.split_once(':')?;
    Some(&key[.."bitmap:".len() + id.len() + 1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sprite(height: u32, src_y: i64, src_height: i64) -> RenderSprite {
        let width = 2;
        let image: Vec<u8> = (0..height)
            .flat_map(|row| std::iter::repeat_n(row as u8, width as usize * 4))
            .collect();
        RenderSprite {
            asset_key: Arc::from("bitmap:9:1"),
            image: Arc::from(image),
            raw_rgba: Some((width, height)),
            bitmap_width: width,
            bitmap_height: height,
            src_x: 0,
            src_y,
            src_width: 2,
            src_height,
            x: 100,
            y: 50,
            ox: 0,
            oy: 3,
            zoom_x: 1.0,
            zoom_y: 2.0,
            opacity: 255,
            angle: 0.0,
            mirror: false,
            color: [0; 4],
            tone: [0; 4],
            blend_type: 0,
            visible: true,
            z: 0,
            tile: true,
            clip_rect: None,
        }
    }

    #[test]
    fn small_bitmaps_are_untouched() {
        let sprites = [sprite(8, 0, 8)];
        assert!(matches!(
            PageCache::default().page(&sprites, 8),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn a_rectangle_inside_one_page_samples_that_page() {
        let sprites = [sprite(25, 12, 3)];
        let paged = PageCache::default().page(&sprites, 10);
        assert_eq!(paged.len(), 1);
        let part = &paged[0];
        assert_eq!(&*part.asset_key, "bitmap:9:1#page1");
        assert_eq!(
            (part.src_y, part.src_height, part.bitmap_height),
            (2, 3, 10)
        );
        assert_eq!(part.raw_rgba, Some((2, 10)));
        assert_eq!(part.oy, 3);
        // the page's first row is source row 10
        assert_eq!(part.image[(2 * 2 * 4) as usize], 12);
    }

    #[test]
    fn a_rectangle_across_pages_is_split_and_stays_in_place() {
        let sprites = [sprite(25, 8, 5)];
        let paged = PageCache::default().page(&sprites, 10);
        let parts: Vec<_> = paged
            .iter()
            .map(|part| {
                (
                    part.asset_key.to_string(),
                    part.src_y,
                    part.src_height,
                    part.oy,
                )
            })
            .collect();
        assert_eq!(
            parts,
            [
                ("bitmap:9:1#page0".to_owned(), 8, 2, 3),
                ("bitmap:9:1#page1".to_owned(), 0, 3, 1),
            ]
        );
        // the last page holds the remaining rows
        let tail = PageCache::default().page(&[sprite(25, 20, 5)], 10)[0].clone();
        assert_eq!((tail.bitmap_height, tail.src_y), (5, 0));
    }

    #[test]
    fn a_new_revision_drops_the_old_pages() {
        let mut cache = PageCache::default();
        cache.page(&[sprite(25, 0, 2)], 10);
        let mut next = sprite(25, 0, 2);
        next.asset_key = Arc::from("bitmap:9:2");
        cache.page(&[next], 10);
        assert_eq!(cache.pages.len(), 1);
        assert!(cache.pages.contains_key("bitmap:9:2"));
    }
}
