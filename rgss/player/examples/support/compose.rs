//! Temporary local debugging: a CPU compositor for render snapshots.
#![allow(dead_code)]
use sabiruby_rgss_core::{RgssRuntime, SpriteSnapshot};
use std::collections::HashMap;

pub fn decode(
    runtime: &RgssRuntime,
    cache: &mut HashMap<String, Option<(u32, u32, Vec<u8>)>>,
    path: &str,
) -> Option<(u32, u32, Vec<u8>)> {
    cache
        .entry(path.to_owned())
        .or_insert_with(|| {
            let bytes = runtime.read_asset(path)?;
            let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
            Some((image.width(), image.height(), image.into_raw()))
        })
        .clone()
}

pub fn draw(
    frame: &mut [u8],
    width: u32,
    height: u32,
    sprite: &SpriteSnapshot,
    pixels: &[u8],
    bw: u32,
) {
    let zx = sprite.zoom_x.max(0.01);
    let zy = sprite.zoom_y.max(0.01);
    let dw = (sprite.src_width as f64 * zx) as i64;
    let dh = (sprite.src_height as f64 * zy) as i64;
    let left = sprite.x - (sprite.ox as f64 * zx) as i64;
    let top = sprite.y - (sprite.oy as f64 * zy) as i64;
    let clip = sprite
        .clip_rect
        .unwrap_or([0, 0, width as i64, height as i64]);
    for dy in 0..dh {
        let ty = top + dy;
        if ty < clip[1].max(0) || ty >= (clip[1] + clip[3]).min(height as i64) {
            continue;
        }
        let sy = sprite.src_y + (dy as f64 / zy) as i64;
        for dx in 0..dw {
            let tx = left + dx;
            if tx < clip[0].max(0) || tx >= (clip[0] + clip[2]).min(width as i64) {
                continue;
            }
            let mut sx = (dx as f64 / zx) as i64;
            if sprite.mirror {
                sx = sprite.src_width - 1 - sx;
            }
            let sx = sprite.src_x + sx;
            let si = ((sy * bw as i64 + sx) * 4) as usize;
            if si + 4 > pixels.len() {
                continue;
            }
            let a = u32::from(pixels[si + 3]) * u32::from(sprite.opacity) / 255;
            if a == 0 {
                continue;
            }
            let ti = ((ty * width as i64 + tx) * 4) as usize;
            for c in 0..3 {
                let s = u32::from(pixels[si + c]);
                let d = u32::from(frame[ti + c]);
                frame[ti + c] = ((s * a + d * (255 - a)) / 255) as u8;
            }
            frame[ti + 3] = 255;
        }
    }
}

/// Draw a snapshot into an RGBA image and save it as PNG.
pub fn save_png(
    runtime: &RgssRuntime,
    cache: &mut HashMap<String, Option<(u32, u32, Vec<u8>)>>,
    path: &std::path::Path,
) -> usize {
    let snapshot = runtime.render_snapshot();
    let (w, h) = (snapshot.width, snapshot.height);
    let mut frame = vec![0u8; (w * h * 4) as usize];
    let mut drawn = 0;
    for sprite in &snapshot.sprites {
        if !sprite.visible || sprite.opacity == 0 || sprite.src_width <= 0 || sprite.src_height <= 0
        {
            continue;
        }
        let (bw, pixels) = match (&sprite.bitmap_rgba, &sprite.bitmap_path) {
            (Some(rgba), _) => (sprite.bitmap_width, rgba.to_vec()),
            (None, Some(path)) => match decode(runtime, cache, path) {
                Some((bw, _, pixels)) => (bw, pixels),
                None => continue,
            },
            _ => continue,
        };
        draw(&mut frame, w, h, sprite, &pixels, bw);
        drawn += 1;
    }
    image::save_buffer(path, &frame, w, h, image::ColorType::Rgba8).unwrap();
    drawn
}
